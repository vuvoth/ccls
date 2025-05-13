# CCLS

A language server for [Circom](https://docs.circom.io/), built with Rust and TypeScript.

## 🚀 Installation

1. **Clone the repository:**
    ```bash
    git clone https://github.com/vuvoth/ccls.git
    cd ccls
    ```

2. **Install Rust** (if not already installed):  
   👉 https://www.rust-lang.org/tools/install

3. **Build or test the project:**
    ```bash
    cargo test     # Run tests
    cargo build    # Build the project
    ```

---

## 🧪 Running Tests (with `insta` snapshots)

Optional, but recommended for snapshot testing.

1. **Install `cargo-insta`:**
    ```bash
    curl -LsSf https://insta.rs/install.sh | sh
    ```

2. **Run the tests:**
    ```bash
    cargo test
    ```

3. **Review snapshot changes:**
    ```bash
    cargo insta review
    ```

📘 More info: [Insta Quickstart](https://insta.rs/docs/quickstart/)

---

## 🐞 Debugging the Extension

1. **Install Circom:**  
   👉 https://docs.circom.io/getting-started/installation/

2. **Install CCLS server and client:**
    ```bash
    cargo xtask install --server
    cargo xtask install --client
    npm audit fix --force   # optional
    ```

3. **Run the extension in VSCode:**
    - Open the `ccls` project in VSCode.
    - Open the *Run and Debug* panel.
    - Select `Run Extension (Debug Build)` and start debugging.

4. A new VSCode window will open.  
   Open a Circom file and try features like **Go to Definition**.

---

