mod entry;
mod output;

use entry::async_main;

// On native platforms, the `main` function
// is called by the executable automatically.
fn main() {
    async_main();
}
