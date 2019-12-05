## Tokio 0.2's basic runtime may lose events.

https://github.com/tokio-rs/tokio/issues/1900

In the same test, tok-pc2 (basic2 runtime 0.2 & chan0.2) will lose most requests,

tok-pc2-with-chan1 (basic2 runtime 0.2 & chan0.1) will lose a small number of requests,

tok-pc1 (basic runtime 0.1 & chan 0.1) and tok-pc1-with-chan2 (basic runtime 0.1 & chan 0.2) will not lose requests.


```sh
:~/a/toktt# cargo run --release --bin tok-pc1
   Compiling toktt v0.1.0 (/xxx/a/toktt)
    Finished release [optimized] target(s) in 2.01s
     Running `target/release/tok-pc1`
2019-12-05 14:01:50.485 INFO [3.tokl] (src/tok-pc1.rs:42) [tok_pc1] -- tok finish: Err(10 secs)
2019-12-05 14:01:50.485 WARN [3.tokl] (src/tok-pc1.rs:46) [tok_pc1] -- tok finish
2019-12-05 14:01:50.486 INFO [1.main] (src/tok-pc1.rs:70) [tok_pc1] -- map: 10000, res: 10000, ac_chan: 10000, ac_req: 10000
[/xxx/.cargo/registry/src/code.aliyun.com-738b7dba08a2a41e/nonblock-logger-0.1.4/src/lib.rs:230] self.join_handle.is_some() = true
:~/a/toktt# cargo run --release --bin tok-pc1-with-chan2
   Compiling toktt v0.1.0 (/xxx/a/toktt)
    Finished release [optimized] target(s) in 2.03s
     Running `target/release/tok-pc1-with-chan2`
2019-12-05 14:02:24.595 INFO [3.tokl] (src/tok-pc1-with-chan2.rs:42) [tok_pc1_with_chan2] -- tok finish: Err(10 secs)
2019-12-05 14:02:24.595 WARN [3.tokl] (src/tok-pc1-with-chan2.rs:46) [tok_pc1_with_chan2] -- tok finish
2019-12-05 14:02:24.596 INFO [1.main] (src/tok-pc1-with-chan2.rs:70) [tok_pc1_with_chan2] -- map: 10000, res: 10000, ac_chan: 10000, ac_req: 10000
[/xxx/.cargo/registry/src/code.aliyun.com-738b7dba08a2a41e/nonblock-logger-0.1.4/src/lib.rs:230] self.join_handle.is_some() = true
:~/a/toktt# cargo run --release --bin tok-pc2
    Finished release [optimized] target(s) in 0.05s
     Running `target/release/tok-pc2`
2019-12-05 14:02:48.463 INFO [3.tokl] (src/tok-pc2.rs:51) [tok_pc2] -- tok finish
2019-12-05 14:02:48.463 INFO [1.main] (src/tok-pc2.rs:75) [tok_pc2] -- map: 10000, res: 5978, ac_chan: 10000, ac_req: 5978
[/xxx/.cargo/registry/src/code.aliyun.com-738b7dba08a2a41e/nonblock-logger-0.1.4/src/lib.rs:230] self.join_handle.is_some() = true
:~/a/toktt# cargo run --release --bin tok-pc2-with-chan1
    Finished release [optimized] target(s) in 0.06s
     Running `target/release/tok-pc2-with-chan1`
2019-12-05 14:03:15.549 INFO [3.tokl] (src/tok-pc2-with-chan1.rs:51) [tok_pc2_with_chan1] -- tok finish
2019-12-05 14:03:15.549 INFO [1.main] (src/tok-pc2-with-chan1.rs:75) [tok_pc2_with_chan1] -- map: 10000, res: 4575, ac_chan: 10000, ac_req: 4575
[/xxx/.cargo/registry/src/code.aliyun.com-738b7dba08a2a41e/nonblock-logger-0.1.4/src/lib.rs:230] self.join_handle.is_some() = true
```
