# Computer use

Agents can drive Windows apps (Outlook, the PstOff GUI) through Codex's native computer use. It sends real mouse and keyboard input and reads windows through UI Automation and window capture.

## Route

- Only Codex sessions have the tool: `mcp__node_repl__js` with the `@oai/sky` package. A Claude agent delegates the clicking to a Codex child through T3's `delegate_task`, and states the scope in the task.
- The `node_repl` tools are deferred: load them through tool search. `mcp__cua_repl` is a separate browser tool, and its line "Native computer APIs are disabled" describes only itself.
- Before the first action, read the computer-use `SKILL.md` under `~/.codex/plugins/cache/openai-bundled/computer-use/<version>/skills/computer-use/` and the docs it marks as required.

## Setup

- The ChatGPT desktop app is running, with **Settings → Computer use → Any App** on.
- After that app restarts or updates, restart T3 Code so new Codex sessions load the config it rewrites.
- Delegate from a T3 thread that has stayed on one provider. Switching a thread's provider breaks its delegation with `parent_not_active` ([t3code#16565](https://github.com/pingdotgg/t3code/issues/16565)), and restarting T3 clears it.

## Guardrails

- Control only windows the agent launched itself, and capture window-scoped screenshots only.
- Apps restore their own state, such as Notepad reopening the user's tabs. Choose targets that hold nothing private.
- Outlook: start it as `outlook.exe /profile PstOffValidation`, the account-free profile from #7. The default `Outlook` profile holds the user's mailbox. Open only copies of public or synthetic PSTs.
- Input goes through the user's real mouse and keyboard, so tell the user before a run.
- Keep screenshots under `.scratch/`.

Whether computer use may replace the manual Outlook check awaits an ADR. Until then that check stays manual (`docs/agents/faithful-checks.md`).

## Smoke test

Launch Notepad, take a window screenshot, type a line, read it back through UI Automation, then close without saving. Notepad keeps unsaved tabs, so close the test tab afterwards.
