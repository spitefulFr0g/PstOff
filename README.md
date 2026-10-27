<p align="center">
  <img src="docs/assets/pstoff-logo.png" alt="PstOff logo" width="200">
</p>

<h1 align="center">PstOff</h1>

<p align="center">An MIT-licensed Windows OST-to-PST converter. Under construction.</p>

PstOff turns an Outlook OST into a conventional Unicode PST, with a CLI and a
guided GUI wizard over the same engine. The source is only ever read; output
is written alongside a Conversion report.

An OST holds only what Outlook synced, so mail that was never cached cannot be
recovered from it.

```text
pstoff inspect source.ost
pstoff convert source.ost --out destination
```

Builds and tests run only inside Windows Sandbox. See
[`docs/agents/environment.md`](docs/agents/environment.md) and
[`docs/agents/safety.md`](docs/agents/safety.md).
