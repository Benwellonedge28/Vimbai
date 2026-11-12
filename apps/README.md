# apps

The user-facing applications.

```
flutter/    the one UI: Android, iOS, web, desktop
```

There used to be two frontends. `web-frontend/` (React) is retired: Flutter is the only UI path,
so screens are migrated into `apps/flutter/` rather than maintained twice. The React sources stay in
the tree as legacy until the last screen is ported, and nothing builds or deploys them any more.

Anything a client talks to lives behind the gateway in `gateway/go/` and the Rust API in
`rust/apps/api-server/`.
