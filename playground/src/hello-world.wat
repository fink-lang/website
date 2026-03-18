;; Placeholder "hello world" program in WASM text format.
;; Used as a stand-in for Fink-compiled WASM while the real codegen is WIP.
;;
;; Uses WASI preview1 for stdout so the same WASI shim used for real
;; Fink output works end-to-end.
(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))

  (memory (export "memory") 1)

  ;; String data starting at byte 16 to leave room for the iovec at 0–7.
  (data (i32.const 16) "Hello from Fink!\n")

  (func (export "_start")
    ;; Build a single iovec at address 0:
    ;;   iov_base = 16  (pointer to the string)
    ;;   iov_len  = 17  (length of "Hello from Fink!\n")
    (i32.store (i32.const 0) (i32.const 16))
    (i32.store (i32.const 4) (i32.const 17))

    ;; fd_write(fd=1 stdout, iov_ptr=0, iov_count=1, nwritten_ptr=8)
    (drop
      (call $fd_write
        (i32.const 1)
        (i32.const 0)
        (i32.const 1)
        (i32.const 8)
      )
    )
  )
)
