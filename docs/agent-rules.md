# Rules for AI Agents

When assisting with the development of the ELSE operating system, AI agents must strictly adhere to the following constraints:

1. **Read Documentation**: Read `docs/architecture.md` and `docs/roadmap.md` before modifying code.
2. **No Unprompted Redesigns**: Never redesign the entire project or change architectural decisions silently to solve a local problem.
3. **Scope Discipline**: Keep changes scoped to the requested phase. Do not implement Phase N+1 features during Phase N.
4. **Isolate Changes**: Do not modify unrelated subsystems.
5. **Preserve Functionality**: Do not delete working functionality without explicit justification.
6. **Test Execution**: Run appropriate tests or builds after modifications. 
7. **Honest Reporting**: Report failures honestly. Never claim QEMU boot success unless QEMU was actually executed successfully and the output was verified.
8. **No Fabrication**: Never invent benchmark results, citations, or research findings.
9. **Unsafe Rust**: Explain newly introduced `unsafe` Rust with explicit safety comments.
10. **Documentation Sync**: Update documentation when an architectural decision changes.
11. **Commit Hygiene**: Prefer small, reviewable commits (if tasked with committing).
