use bevy::math::Vec2;

fn fill_random(bytes: &mut [u8]) {
    getrandom::fill(bytes).expect("randomness unavailable");
}

pub fn random_u64() -> u64 {
    let mut bytes = [0_u8; 8];
    fill_random(&mut bytes);
    u64::from_le_bytes(bytes)
}

fn random_unit_f64() -> f64 {
    const SCALE: f64 = 1.0 / ((u64::MAX as f64) + 1.0);
    (random_u64() as f64) * SCALE
}

pub fn random_bool(probability: f64) -> bool {
    debug_assert!((0.0..=1.0).contains(&probability));
    if probability <= 0.0 {
        return false;
    }
    if probability >= 1.0 {
        return true;
    }
    random_unit_f64() < probability
}

pub fn random_f32() -> f32 {
    random_unit_f64() as f32
}

pub fn random_range_usize(range: std::ops::Range<usize>) -> usize {
    assert!(range.start < range.end, "empty usize range");
    let width = range.end - range.start;
    range.start + random_index(width)
}

pub fn random_range_i32(range: std::ops::Range<i32>) -> i32 {
    assert!(range.start < range.end, "empty i32 range");
    let width = (range.end - range.start) as usize;
    range.start + random_index(width) as i32
}

pub fn random_range_i32_inclusive(range: std::ops::RangeInclusive<i32>) -> i32 {
    let start = *range.start();
    let end = *range.end();
    assert!(start <= end, "invalid inclusive i32 range");
    start + random_index((end - start + 1) as usize) as i32
}

pub fn random_range_f32(range: std::ops::Range<f32>) -> f32 {
    assert!(range.start < range.end, "empty f32 range");
    range.start + ((range.end - range.start) * random_f32())
}

pub fn random_index(len: usize) -> usize {
    assert!(len > 0, "cannot pick from empty slice");
    let zone = u64::MAX - (u64::MAX % len as u64);
    loop {
        let value = random_u64();
        if value < zone {
            return (value % len as u64) as usize;
        }
    }
}

pub fn shuffle<T>(slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }
    for idx in (1..slice.len()).rev() {
        let swap_idx = random_index(idx + 1);
        slice.swap(idx, swap_idx);
    }
}

pub fn random_vec2(range: std::ops::Range<f32>) -> Vec2 {
    let x = random_range_f32(range.clone());
    let y = random_range_f32(range);
    Vec2::new(x, y)
}
