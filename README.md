# Amnezichat_Server

<img src="banner.png" width="1200">

<!-- INSTALLATION -->
## Server setup:

    sudo apt update
    sudo apt install curl build-essential git
    curl https://sh.rustup.rs -sSf | sh -s -- -y
    git clone https://git.disroot.org/UmutCamliyurt/Amnezichat_Server.git
    cd Amnezichat_Server
    cargo build --release
    cargo run --release

## Server setup with Docker:
    
    sudo apt update
    sudo apt install docker.io git
    git clone https://git.disroot.org/UmutCamliyurt/Amnezichat_Server.git
    cd Amnezichat_Server
    docker build --network=host -t amnezichat_server:latest .
    docker run --network=host amnezichat_server:latest

## Requirements:

- [Rust](https://www.rust-lang.org), [Tor](https://gitlab.torproject.org/tpo/core/tor)

<!-- MIRRORS -->
## Git Mirrors

You can access **Amnezichat_Server** source code from multiple mirror repositories:

- 🔗 **[Disroot Main Repository](https://git.disroot.org/UmutCamliyurt/Amnezichat_Server)**
- 🔗 **[GitHub Mirror](https://github.com/umutcamliyurt/Amnezichat_Server)**

<!-- LICENSE -->
## License

Distributed under the GPLv3 License. See `LICENSE` for more information.

## <a href="CONTRIBUTORS.md">Contributors</a>

## Donate to support development of this project!

**Monero(XMR):** 88a68f2oEPdiHiPTmCc3ap5CmXsPc33kXJoWVCZMPTgWFoAhhuicJLufdF1zcbaXhrL3sXaXcyjaTaTtcG1CskB4Jc9yyLV

**Bitcoin(BTC):** bc1qn42pv68l6erl7vsh3ay00z8j0qvg3jrg2fnqv9
