#![no_std]
#![no_main]

use userspace::{sys_log, sys_map_phys, sys_recv, sys_send, Message, CAP_NETWORK_SERVICE_EP, CAP_NIC_DRIVER_EP, CAP_PACKET_BUFFER};

pub const MSG_NET_RX: u64 = 100;
pub const MSG_NET_TX: u64 = 101;
pub const MSG_NET_TX_ACK: u64 = 102;
pub const MSG_UDP_BIND: u64 = 103;
pub const MSG_UDP_SEND: u64 = 104;
pub const MSG_UDP_RECV: u64 = 105;

const OUR_IP: [u8; 4] = [10, 0, 2, 15];
const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];

static mut OUR_MAC: [u8; 6] = [0; 6];
static mut GATEWAY_MAC: [u8; 6] = [0; 6];
static mut GATEWAY_MAC_RESOLVED: bool = false;

static mut BOUND_PORT: u16 = 0;
static mut BOUND_EP: usize = 0;

static mut SHARED_BUF: *mut u8 = core::ptr::null_mut();

fn compute_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < data.len() {
        let word = if i + 1 < data.len() {
            ((data[i] as u32) << 8) | (data[i + 1] as u32)
        } else {
            (data[i] as u32) << 8
        };
        sum = sum.wrapping_add(word);
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn send_arp_request() {
    sys_log("[Net] Sending ARP request for gateway...");
    unsafe {
        let our_mac = OUR_MAC;
        let tx_buf = core::slice::from_raw_parts_mut(SHARED_BUF.add(2048), 60);
        
        // Dest MAC: Broadcast
        tx_buf[0..6].copy_from_slice(&[0xFF; 6]);
        // Src MAC
        tx_buf[6..12].copy_from_slice(&our_mac);
        // EtherType: ARP (0x0806)
        tx_buf[12] = 0x08; tx_buf[13] = 0x06;
        
        // ARP Header
        tx_buf[14] = 0x00; tx_buf[15] = 0x01; // Hardware: Ethernet
        tx_buf[16] = 0x08; tx_buf[17] = 0x00; // Protocol: IPv4
        tx_buf[18] = 0x06; // Hw size
        tx_buf[19] = 0x04; // Proto size
        tx_buf[20] = 0x00; tx_buf[21] = 0x01; // Opcode: Request
        
        tx_buf[22..28].copy_from_slice(&our_mac); // Sender MAC
        tx_buf[28..32].copy_from_slice(&OUR_IP); // Sender IP
        
        tx_buf[32..38].copy_from_slice(&[0x00; 6]); // Target MAC
        tx_buf[38..42].copy_from_slice(&GATEWAY_IP); // Target IP
        
        // Send TX request
        let tx_msg = Message { rdi: MSG_NET_TX, rsi: 42, rdx: 0, r10: 0, r8: 0, r9: 0 };
        sys_send(CAP_NIC_DRIVER_EP, &tx_msg);
    }
}

fn handle_arp(packet: &[u8]) {
    if packet.len() < 28 { return; }
    let opcode = (packet[6] as u16) << 8 | (packet[7] as u16);
    if opcode == 2 { // Reply
        let mut sender_ip = [0u8; 4];
        sender_ip.copy_from_slice(&packet[14..18]);
        if sender_ip == GATEWAY_IP {
            unsafe {
                GATEWAY_MAC = [
                    packet[8], packet[9], packet[10],
                    packet[11], packet[12], packet[13]
                ];
                GATEWAY_MAC_RESOLVED = true;
            }
            sys_log("[Net] Gateway ARP resolved!");
            // We should process queued UDP packets here if we had a queue
        }
    } else if opcode == 1 { // Request
        let mut target_ip = [0u8; 4];
        target_ip.copy_from_slice(&packet[24..28]);
        if target_ip == OUR_IP {
            sys_log("[Net] Replying to ARP request...");
            // Construct ARP reply
            unsafe {
                let our_mac = OUR_MAC;
                let tx_buf = core::slice::from_raw_parts_mut(SHARED_BUF.add(2048), 60);
                
                // Dest MAC: Sender MAC
                tx_buf[0..6].copy_from_slice(&packet[8..14]);
                tx_buf[6..12].copy_from_slice(&our_mac);
                tx_buf[12] = 0x08; tx_buf[13] = 0x06;
                
                tx_buf[14] = 0x00; tx_buf[15] = 0x01;
                tx_buf[16] = 0x08; tx_buf[17] = 0x00;
                tx_buf[18] = 0x06; tx_buf[19] = 0x04;
                tx_buf[20] = 0x00; tx_buf[21] = 0x02; // Reply
                
                tx_buf[22..28].copy_from_slice(&our_mac);
                tx_buf[28..32].copy_from_slice(&OUR_IP);
                tx_buf[32..38].copy_from_slice(&packet[8..14]);
                tx_buf[38..42].copy_from_slice(&packet[14..18]);
                
                let tx_msg = Message { rdi: MSG_NET_TX, rsi: 42, rdx: 0, r10: 0, r8: 0, r9: 0 };
                sys_send(CAP_NIC_DRIVER_EP, &tx_msg);
            }
        }
    }
}

fn handle_ipv4(packet: &[u8]) {
    if packet.len() < 20 { return; }
    let ihl = (packet[0] & 0x0F) as usize * 4;
    let protocol = packet[9];
    let mut dest_ip = [0u8; 4];
    dest_ip.copy_from_slice(&packet[16..20]);
    
    if dest_ip != OUR_IP && dest_ip != [255, 255, 255, 255] {
        return;
    }
    
    if protocol == 17 { // UDP
        let udp_data = &packet[ihl..];
        handle_udp(udp_data, &packet[12..16]);
    }
}

fn handle_udp(packet: &[u8], src_ip: &[u8]) {
    if packet.len() < 8 { return; }
    let src_port = ((packet[0] as u16) << 8) | (packet[1] as u16);
    let dest_port = ((packet[2] as u16) << 8) | (packet[3] as u16);
    let length = (((packet[4] as u16) << 8) | (packet[5] as u16)) as usize;
    
    if length > packet.len() || length < 8 { return; }
    
    let payload_len = length - 8;
    
    unsafe {
        if BOUND_PORT == dest_port && BOUND_EP != 0 {
            sys_log("[Net] Delivering UDP payload to app!");
            // Copy payload to App buffer (we'll just use offset 3072 in the same shared buffer)
            let dst_ptr = SHARED_BUF.add(3072);
            core::ptr::copy_nonoverlapping(packet.as_ptr().add(8), dst_ptr, payload_len);
            
            let src_ip_u32 = u32::from_be_bytes(src_ip.try_into().unwrap());
            
            let msg = Message {
                rdi: MSG_UDP_RECV,
                rsi: payload_len as u64,
                rdx: src_ip_u32 as u64,
                r10: src_port as u64,
                r8: 0, r9: 0
            };
            sys_send(BOUND_EP, &msg);
        }
    }
}

fn send_udp(dest_ip: u32, dest_port: u16, src_port: u16, payload_len: usize) {
    unsafe {
        if !GATEWAY_MAC_RESOLVED {
            sys_log("[Net] Cannot send UDP, gateway MAC unresolved! Dropping packet.");
            return;
        }
        
        let our_mac = OUR_MAC;
        let gateway_mac = GATEWAY_MAC;
        
        let tx_buf = core::slice::from_raw_parts_mut(SHARED_BUF.add(2048), 1500);
        
        // Ethernet Header (14 bytes)
        tx_buf[0..6].copy_from_slice(&gateway_mac);
        tx_buf[6..12].copy_from_slice(&our_mac);
        tx_buf[12] = 0x08; tx_buf[13] = 0x00; // IPv4
        
        // IPv4 Header (20 bytes)
        let total_len = 20 + 8 + payload_len;
        tx_buf[14] = 0x45; // Version 4, IHL 5
        tx_buf[15] = 0x00; // DSCP/ECN
        tx_buf[16] = (total_len >> 8) as u8; tx_buf[17] = total_len as u8; // Total len
        tx_buf[18] = 0x00; tx_buf[19] = 0x00; // ID
        tx_buf[20] = 0x40; tx_buf[21] = 0x00; // Flags/Frag (Don't fragment)
        tx_buf[22] = 64; // TTL
        tx_buf[23] = 17; // Protocol UDP
        tx_buf[24] = 0x00; tx_buf[25] = 0x00; // Checksum placeholder
        tx_buf[26..30].copy_from_slice(&OUR_IP);
        let dest_ip_bytes = dest_ip.to_be_bytes();
        tx_buf[30..34].copy_from_slice(&dest_ip_bytes);
        
        let ip_csum = compute_checksum(&tx_buf[14..34]);
        tx_buf[24] = (ip_csum >> 8) as u8; tx_buf[25] = ip_csum as u8;
        
        // UDP Header (8 bytes)
        tx_buf[34] = (src_port >> 8) as u8; tx_buf[35] = src_port as u8;
        tx_buf[36] = (dest_port >> 8) as u8; tx_buf[37] = dest_port as u8;
        let udp_len = 8 + payload_len;
        tx_buf[38] = (udp_len >> 8) as u8; tx_buf[39] = udp_len as u8;
        tx_buf[40] = 0x00; tx_buf[41] = 0x00; // Checksum optional in IPv4, set to 0
        
        // Payload was placed at offset 3072 by the app
        let app_payload = core::slice::from_raw_parts(SHARED_BUF.add(3072), payload_len);
        tx_buf[42..42+payload_len].copy_from_slice(app_payload);
        
        let tx_msg = Message { rdi: MSG_NET_TX, rsi: (14 + total_len) as u64, rdx: 0, r10: 0, r8: 0, r9: 0 };
        sys_send(CAP_NIC_DRIVER_EP, &tx_msg);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[Net] Network Service started");
    
    let shared_buf_virt = sys_map_phys(CAP_PACKET_BUFFER, core::u64::MAX, core::u64::MAX);
    if shared_buf_virt == 0 {
        sys_log("[Net] Failed to map CAP_PACKET_BUFFER");
        loop {}
    }
    
    unsafe {
        SHARED_BUF = shared_buf_virt as *mut u8;
    }
    
    let mut msg = Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
    
    loop {
        sys_recv(CAP_NETWORK_SERVICE_EP, &mut msg);
        
        match msg.rdi {
            MSG_NET_RX => {
                if msg.rsi == 0xFFFFFFFF {
                    // Initial MAC address from driver
                    let mac_val = msg.rdx;
                    unsafe {
                        OUR_MAC[0] = (mac_val >> 40) as u8;
                        OUR_MAC[1] = (mac_val >> 32) as u8;
                        OUR_MAC[2] = (mac_val >> 24) as u8;
                        OUR_MAC[3] = (mac_val >> 16) as u8;
                        OUR_MAC[4] = (mac_val >> 8) as u8;
                        OUR_MAC[5] = mac_val as u8;
                    }
                    sys_log("[Net] Received MAC address from driver");
                    send_arp_request();
                } else {
                    let length = msg.rsi as usize;
                    unsafe {
                        let packet = core::slice::from_raw_parts(SHARED_BUF, length);
                        if length >= 14 {
                            let eth_type = ((packet[12] as u16) << 8) | (packet[13] as u16);
                            match eth_type {
                                0x0806 => handle_arp(&packet[14..]),
                                0x0800 => handle_ipv4(&packet[14..]),
                                _ => {}
                            }
                        }
                    }
                }
            },
            MSG_NET_TX_ACK => {
                // Ignore for now
            },
            MSG_UDP_BIND => {
                unsafe {
                    BOUND_PORT = msg.rsi as u16;
                    BOUND_EP = msg.rdx as usize; // Which endpoint to notify
                }
                sys_log("[Net] Application bound to UDP port");
            },
            MSG_UDP_SEND => {
                let dest_ip = msg.rsi as u32;
                let dest_port = msg.rdx as u16;
                let src_port = msg.r10 as u16;
                let payload_len = msg.r8 as usize;
                send_udp(dest_ip, dest_port, src_port, payload_len);
            },
            _ => {}
        }
    }
}
