//! ccls binary entry point — delegates to the `ccls` library.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    ccls::run()
}
