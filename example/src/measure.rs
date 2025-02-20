use crate::print_fit;

/// Prints right away when an instance is dropped.
pub struct Dropper {
    pub name: String,
}

impl Drop for Dropper {
    fn drop(&mut self) {
        print_fit!("Dropper \"{}\" has been dropped", self.name);
    }
}
