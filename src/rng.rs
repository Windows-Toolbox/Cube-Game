//! Random numbers – what Python's `random` module did for the game: a fast generator (xoshiro256**) with the
//! functions the game uses. Tests and the bot can start it from a fixed seed.

use std::cell::RefCell;

pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn seeded(seed: u64) -> Rng {
        // splitmix64 spreads the seed over the state
        let mut z = seed;
        let mut next = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        Rng { s: [next(), next(), next(), next()] }
    }
    pub fn from_time() -> Rng {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Rng::seeded(t ^ std::process::id() as u64)
    }
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }
    /// `random.random()`: 0 ≤ x < 1.
    pub fn random(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// `random.uniform(a, b)`.
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }
    /// `random.randint(a, b)`: both ends included.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        if b <= a {
            return a;
        }
        a + (self.next_u64() % ((b - a + 1) as u64)) as i64
    }
    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.random() < p
    }
    /// `random.choice`: the index of one element (`None` for an empty list).
    pub fn index(&mut self, len: usize) -> Option<usize> {
        if len == 0 { None } else { Some(self.randint(0, len as i64 - 1) as usize) }
    }
    /// `random.choices(items, weights)[0]`: the index of one element, by weight.
    pub fn weighted(&mut self, weights: &[f64]) -> Option<usize> {
        let total: f64 = weights.iter().filter(|w| **w > 0.0).sum();
        if total <= 0.0 {
            return None;
        }
        let mut x = self.random() * total;
        for (i, w) in weights.iter().enumerate() {
            if *w <= 0.0 {
                continue;
            }
            if x < *w {
                return Some(i);
            }
            x -= w;
        }
        weights.iter().rposition(|w| *w > 0.0)
    }
    /// `random.shuffle`.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.randint(0, i as i64) as usize;
            items.swap(i, j);
        }
    }
    /// `random.sample(range(n), k)`: `k` different indexes.
    pub fn sample(&mut self, n: usize, k: usize) -> Vec<usize> {
        let mut all: Vec<usize> = (0..n).collect();
        self.shuffle(&mut all);
        all.truncate(k);
        all
    }
}

thread_local! {
    static RNG: RefCell<Rng> = RefCell::new(Rng::from_time());
}

/// The game's generator (one per thread – the game runs on one).
pub fn with<R>(f: impl FnOnce(&mut Rng) -> R) -> R {
    RNG.with(|r| f(&mut r.borrow_mut()))
}

#[cfg(test)]
/// For tests and the bot: the same numbers every time.
pub fn seed(seed: u64) {
    RNG.with(|r| *r.borrow_mut() = Rng::seeded(seed));
}

pub fn random() -> f64 {
    with(|r| r.random())
}
pub fn uniform(a: f64, b: f64) -> f64 {
    with(|r| r.uniform(a, b))
}
pub fn chance(p: f64) -> bool {
    with(|r| r.chance(p))
}
pub fn weighted(weights: &[f64]) -> Option<usize> {
    with(|r| r.weighted(weights))
}
pub fn index(len: usize) -> Option<usize> {
    with(|r| r.index(len))
}
pub fn sample(n: usize, k: usize) -> Vec<usize> {
    with(|r| r.sample(n, k))
}
