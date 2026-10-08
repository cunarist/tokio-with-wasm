// The bootstrap script that every blocking web worker of `tokio_with_wasm`
// runs. The first message carries the glue path together with the wasm
// module and memory, so this file works for any application as it is.
self.onmessage = event => {
  globalThis.isBlockingTokioThread = true;
  let initialised = import(event.data.glue_path)
    .then(async wasmBindings => {
      globalThis.wasmBindings = wasmBindings;
      globalThis.wasmExports = await wasmBindings.default(event.data);
      return wasmBindings;
    })
    .catch(err => {
      // Propagate to main `onerror`:
      setTimeout(() => {
        throw err;
      });
      // Rethrow to keep promise rejected
      // and prevent execution of further commands:
      throw err;
    });

  self.onmessage = async event => {
    // This will queue further commands up
    // until the module is fully initialised:
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
      // A panicking task traps here. Throwing inside an async
      // handler would only reject its promise, which the parent
      // thread never sees, so the error is rethrown from a timeout
      // to reach the `Worker`'s `onerror`:
      setTimeout(() => {
        throw err;
      });
      throw err;
    }
  };
};
