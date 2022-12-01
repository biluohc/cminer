fn main() {
    fun()
}
/*
let hashser = self.hasher.clone()
1.60ns
let hash = .finalize_with_nonce(nonce);
2.2.185µs
let hash = self.matrix.heavy_hash(hash);
3.3.866µs
Uint256::from_le_bytes(hash.as_bytes())

并且绝大部分都在 keccak::f1600, 
keccak(520m)性能还可以,asm(660m)
keccak-tiny(480m)不用考虑,
keccak-p(502m)代码似乎简单

debug.1c: 14.7k
relea.1c: 1.11m
3070-8g: 359.38m
参考 md5 avx2 4x, avx512 8x
*/

fn fun() {
    use pow::{target2difficulty, State};

    let powhash =
        "7d92a563859e13119221f1a288615330a05d786a9cabc1b997c72fe9f6aa37e4edcfaecb84010000";
    let pow = State::with_powhash(powhash, 100000).unwrap();

    let start = std::time::Instant::now();
    let mut loops = 0;
    loop {
        for _ in 0..8196u64 {
            loops += 1;
            let i = pow.calculate_pow(loops);
            if i < pow.target {
                let diff = target2difficulty(i);
                println!("{} {}: {:x}", &powhash[..6], diff, i)
            }
        }

        if start.elapsed().as_secs() > 30 {
            break;
        }
    }

    let hashrate = loops as f64 / start.elapsed().as_secs_f64();
    println!("{}/s {}", hashrate, loops);
}
