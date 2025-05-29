use std::cell::LazyCell;

thread_local! {
  pub static IS_MAIN_THREAD: LazyCell<bool> =
    LazyCell::new(|| {
      #[cfg(not(feature = "non_browser"))] {
        web_sys::window().is_some()
      }
      #[cfg(feature = "non_browser")] {
        true
      }
    });
}

pub fn is_main_thread() -> bool {
  let mut is_main: bool = false;
  IS_MAIN_THREAD.with(|cell| {
    is_main = **cell;
  });
  is_main
}
