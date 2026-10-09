// The bootstrap script that every blocking web worker of `tokio_with_wasm`
// runs. The first message carries the glue path together with the wasm
// module and memory, so this file works for any application as it is.

// A rejected promise goes unnoticed by the parent thread,
// so errors are rethrown from a timeout to reach the `Worker`'s `onerror`.
const report = err => setTimeout(() => {
  throw err;
});

self.onmessage = event => {
  globalThis.isBlockingTokioThread = true;
  let wasmExports;
  const initialised = import(event.data.glue_path).then(async wasmBindings => {
    wasmExports = await wasmBindings.default(event.data);
    return wasmBindings;
  });
  initialised.catch(report);

  self.onmessage = async event => {
    // Further messages wait until the module is initialised.
    const wasmBindings = await initialised;
    if (event.data === null) {
      // The pool lets this worker go. Its stack lives in the shared memory,
      // so it is freed before the worker closes.
      wasmExports.__wbindgen_thread_destroy?.();
      close();
      return;
    }
    try {
      wasmBindings.task_worker_entry_point(event.data);
    } catch (err) {
      // A panicking task traps here.
      report(err);
    }
  };
};
