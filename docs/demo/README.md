# Local output capture

Captured on 23 October 2026 from base revision `a45fefe2754921983cd7cd35a7824f3f7fdcccb5` with the documentation and demo additions in this commit.

Rust 1.96.0 on Linux ARM64. cargo build -p ncal passed. cargo test --workspace completed with zero tests. The demo runs the actual CLI with explicit fictional environment values, which take precedence over credential stores. auth status displays local metadata and makes no API request; it does not verify the fictional token. No real session, calendar data or secret was read. The PNG is a rendered transcript.

Commands from the repository root:

```sh
sh docs/demo/run.sh
```

Image: [status.png](status.png)
