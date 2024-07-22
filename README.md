This repository contains an implementation of an LSP (Language server protocol) in Rust
for the programming language SQF (Arma 3).

The Rust crate supporting this implementation can be found [here](https://github.com/sqf-analyzer/sqf-analyzer).

See [client/README.md](client/README.md) for a summary of current functionality.

## How to develop

```bash
cargo build --release

curl -fsSL https://get.pnpm.io/install.sh | sh -
curl -fsSL https://fnm.vercel.app/install | bash
fnm use --install-if-missing 20
source /root/.bashrc
cd client
pnpm i
```

* Open [`example.sqf`](./example.sqf)
* press <kbd>F5</kbd> or change to the Debug panel and click <kbd>Launch Client</kbd>

## How to publish

Create a new semver tag from main and push it.

# Authors

* Everything inside `src`: Lord Golias
* Everything outside `src`: [IWANABETHATGUY](https://github.com/IWANABETHATGUY/tower-lsp-boilerplate)
