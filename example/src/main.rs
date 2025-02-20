mod entry;
mod measure;
mod output;

use entry::*;
use measure::*;

// On native platforms, the `main` function
// is called by the executable automatically.
fn main() {
    async_main();
}
