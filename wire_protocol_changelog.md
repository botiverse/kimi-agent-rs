# Wire Protocol Changelog

## 1.10 Advertisement

`kimi-agent-rs` now advertises wire protocol `1.10` to match upstream `kimi-cli`.

This version bump is intentionally narrower than full upstream feature parity. Per Lane #9a
ratification (`d247ffd1`) and Ruling 5 (`f4356695`), RS continues to treat the following
upstream tokens as accepted divergences and does not emit them:

- `MCPLoadingBegin` / `MCPLoadingEnd`
  - Upstream uses these as MCP loading progress signals.
  - Lane #9a classified them as UI-only / presentation-layer tokens; upstream ACP already
    pass-ignores them.
- `BtwBegin` / `BtwEnd`
  - Upstream emits these around `/btw` side-question handling.
  - RS does not implement `/btw`, so these remain feature-coupled non-emissions.
- `HookTriggered` / `HookResolved`
  - Upstream emits these around hook-engine execution.
  - RS does not implement the corresponding hook engine, so these remain feature-coupled
    non-emissions.

This file is the reviewer-facing annotation requested for the `1.2 -> 1.10` bump so downstream
consumers do not infer that every upstream `1.10` token is emitted by RS.

Rationale source for the accepted-divergence classification:
- Lane #9a §11 / Decision 1 resolution chain:
  `5d9e2e68 -> 9629cbca -> eaa8c71b -> f4356695`
