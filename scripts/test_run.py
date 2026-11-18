import subprocess
import time
import socket
import select
import sys

print("=> Launching ELSE OS in QEMU...")
qemu = subprocess.Popen([
    "qemu-system-x86_64",
    "-cdrom", "else.iso",
    "-drive", "file=disk.img,format=raw,index=0,media=disk",
    "-netdev", "user,id=net0,hostfwd=udp::8080-:8080",
    "-device", "rtl8139,netdev=net0",
    "-serial", "stdio",
    "-display", "none",
    "-no-reboot"
], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setblocking(False)

print("=> Waiting for ELSE to boot and initialize...")
time.sleep(8)

received = False
print("=> Testing host <-> ELSE UDP ping/pong...")
for attempt in range(1, 10):
    print(f"   [Attempt {attempt}] Sending 'ELSE_PING' to 127.0.0.1:8080...")
    sock.sendto(b"ELSE_PING", ("127.0.0.1", 8080))
    
    r, _, _ = select.select([sock], [], [], 1.5)
    if r:
        data, addr = sock.recvfrom(1024)
        print(f"\n=> SUCCESS! Received UDP response from ELSE: {data.decode()}\n")
        received = True
        break
    time.sleep(0.5)

if not received:
    print("=> No response received within timeout.")

time.sleep(1)
qemu.terminate()
try:
    out, _ = qemu.communicate(timeout=3)
except Exception:
    qemu.kill()
    out, _ = qemu.communicate()

print("="*60)
print("ELSE CONSOLE SERIAL LOG")
print("="*60)
print(out)
