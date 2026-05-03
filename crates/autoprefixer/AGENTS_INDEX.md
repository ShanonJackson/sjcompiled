# Agents index — autoprefixer wrap-up

The controller agent (this conversation's owner) does not write code.
The controller delegates each unit to a subagent via the Agent tool,
reviews the diff + the agent's `AGENT_X_DONE.md` report, and updates
shared docs (HANDOVER.md, MORNING.md, STATUS.md) based on what landed.

Each `AGENT_X.md` is a self-contained prompt. Subagents start with NO
memory of the controller's conversation — the prompt + the docs it
points at are their full briefing.

## Dependency graph

```
AGENT_1 (Prefixes::new + entry shell)        ← UNBLOCKED, foundation
   │
   ├── AGENT_2 (supports.rs)                 ← can run in PARALLEL with AGENT_3
   │      │     once AGENT_1's signature lands
   │      │
   ├── AGENT_3 (transition.rs)               ← can run in PARALLEL with AGENT_2
   │      │     once AGENT_1's signature lands
   │      │
   └── AGENT_4 (processor.rs — the engine)   ← needs AGENT_1's body landed
          │     ideally also AGENT_2 + AGENT_3
          │     2–3 sessions on its own
          │
          └── AGENT_5 (AFM hack instrumentation + hack subset port)
                 │     Phase A: instrumentation report
                 │     Phase B: port the hacks AFM uses
                 │
                 └── AGENT_6 (parity-runner stage + NAPI wire-in)
                              the FINAL unit; touches workspace-shared
                              files (must ASK USER before editing)
```

## Parallelism opportunities

- **AGENT_2 + AGENT_3** in parallel after AGENT_1's `Prefixes::new`
  signature is locked (body can still be in progress).
- **AGENT_5 Phase A** can START in parallel with AGENT_4 — the
  instrumentation report is independent of `processor.rs` body.
  Phase B blocks on AGENT_4.
- **AGENT_6** is sequential, last.

## Sequential critical path

`AGENT_1 body → AGENT_4 body → AGENT_5 Phase B → AGENT_6`. ~5–8
sessions even with maximum subagent fanout on the parallel pieces.

## Running an agent

The controller invokes via the Agent tool with `subagent_type: general-purpose`,
optionally `isolation: worktree` for isolated branches. The full prompt
to pass is the contents of `AGENT_X.md` — no edits, no abridgements.
The subagent works in the worktree (or main tree), runs sign-off gates
itself, writes `AGENT_X_DONE.md`, and reports back.

The controller then:
1. Reads `AGENT_X_DONE.md`.
2. Diffs the worktree (if isolated) or `git diff` if main.
3. Verifies sign-off gates pass.
4. Folds the agent's findings into `HANDOVER.md` (especially §11
   "JS quirks") and `STATUS.md` (test count, Phase 7 checklist).
5. Updates `MORNING.md` with the next-session handoff.

## What lives where

| File | Owner | Purpose |
|---|---|---|
| `AGENT_1.md` | controller (writes) → AGENT_1 (reads) | Prefixes::new prompt |
| `AGENT_2.md` | controller → AGENT_2 | supports.rs prompt |
| `AGENT_3.md` | controller → AGENT_3 | transition.rs prompt |
| `AGENT_4.md` | controller → AGENT_4 | processor.rs prompt |
| `AGENT_5.md` | controller → AGENT_5 | hack instrumentation + port prompt |
| `AGENT_6.md` | controller → AGENT_6 | parity-runner + NAPI wire-in prompt |
| `AGENT_X_DONE.md` | each agent (writes) → controller (reads) | per-agent close-out report |
| `AFM_HACKS_INSTRUMENTATION.md` | AGENT_5 (writes) | empirical hack-frequency report |
| `HANDOVER.md` | controller (maintains) | exhaustive permanent reference |
| `MORNING.md` | controller (rewrites each session) | next-session handoff |
| `STATUS.md` (`crates/STATUS.md`) | controller (updates per session) | workspace-wide phase tracker |
