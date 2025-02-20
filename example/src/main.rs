mod entry;
mod measure;
mod output;

pub(crate) use entry::*;
pub(crate) use measure::*;

// On native platforms, the `main` function
// is called by the executable automatically.
fn main() {
    async_main();
}
