use super::{age, level};

#[test]
fn each_level_is_read_as_the_age_its_british_certificates_are_for() {
    assert_eq!(
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 15].map(age),
        [0, 7, 7, 7, 8, 12, 12, 15, 18, 18, 18, 21].map(Some)
    );
    assert_eq!(age(12), None);
    assert_eq!(age(0), None);
}

#[test]
fn an_age_is_written_as_the_highest_level_read_at_or_below_it() {
    assert_eq!(
        [0, 6, 7, 8, 11, 12, 14, 15, 17, 18, 20, 21, 99].map(level),
        [1, 1, 4, 5, 5, 7, 7, 8, 8, 11, 11, 15, 15]
    );
}

#[test]
fn a_level_written_for_an_age_reads_back_at_or_below_it() {
    for limit in 0..=25 {
        assert!(age(level(limit)).is_some_and(|read| read <= limit));
    }
}
