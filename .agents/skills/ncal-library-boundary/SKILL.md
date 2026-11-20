---
name: ncal-library-boundary
description: Maintain the boundary between the reusable calendar libraries, their local CLI, and hosted consumers.
---

Keep ncal useful as a standalone local tool. Dependencies flow from the ncal
binary through ncal-mcp and ncal-core to ncal-api. Add calendar capabilities
to ncal-core first, then expose them through thin CLI and MCP adapters.

Keep tenants, browser sessions, cookies, OAuth authorization servers, and
deployment out of this repository. Library dependency trees must exclude
axum, sqlx, idclient, hyper-server, and tower-http. Library sources must
avoid Microapps hostnames.

Apply SemVer to public Rust APIs and MCP tool names and argument schemas.
Document breaking changes in the changelog and bump the minor version
while the project is at 0.x.
