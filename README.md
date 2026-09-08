# Docker Dashboard

A Dashboard written in Rust using egui for viewing Docker hosts, containers, images, and Compose data in a single window.

## Run

```bash
cargo run -p docker-terminal
```

Requires Rust 1.88+ (see `rust-toolchain.toml`) and, for live data in later steps, Docker on `PATH`.

## Mock stack

Fast Alpine Compose services (`api`, `worker`, `db`, `unhealthy`) for exercising the dashboard. `db` writes into a named volume. `unused` is built but not started so Storage Details has an unused image.

```bash
docker compose -f testdata/mock-stack/docker-compose.yml --profile disk build unused
docker compose -f testdata/mock-stack/docker-compose.yml up -d --build
```

Tear down with `docker compose -f testdata/mock-stack/docker-compose.yml down -v`. The API is published on `localhost:18080`.
