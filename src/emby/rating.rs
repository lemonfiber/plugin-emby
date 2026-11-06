const LADDER: [(u32, u32); 12] = [
    (1, 0),
    (2, 7),
    (3, 7),
    (4, 7),
    (5, 8),
    (6, 12),
    (7, 12),
    (8, 15),
    (9, 18),
    (10, 18),
    (11, 18),
    (15, 21),
];

pub(super) fn age(level: u32) -> Option<u32> {
    LADDER
        .iter()
        .find(|(at, _)| *at == level)
        .map(|(_, age)| *age)
}

pub(super) fn level(age: u32) -> u32 {
    LADDER
        .iter()
        .filter(|(_, at)| *at <= age)
        .map(|(level, _)| *level)
        .max()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
