# p2piroh

Minimal Rust + iroh + WebAssembly collaborative text field.

- Runs a local web app (`http://...`) for phone/PC browser.
- Uses iroh P2P (with relay + hole punching) between two Rust peers, so it works across NATs.
- Any edit is broadcast immediately to the other side.

## 1) One-time setup

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

## 2) Build wasm frontend

```bash
wasm-pack build wasm --target web --release --out-dir static/pkg --out-name app
```

## 3) Run side A (host)

```bash
cargo run -p p2piroh
```

- Copy the printed ticket.
- Open the printed `http://...` URL in that machine's browser (or phone on same LAN).

## 4) Run side B (join)

```bash
cargo run -p p2piroh -- --join '<PASTE_TICKET>'
```

- Open side B's `http://...` URL in browser.
- Typing in either text area updates the other in realtime.

## Notes

- Default HTTP bind is `0.0.0.0:8080`; override with `--http`.
- Example for phone access from your PC on LAN:

```bash
cargo run -p p2piroh -- --http 0.0.0.0:8080
```

Then browse from phone to `http://<pc-lan-ip>:8080`.
