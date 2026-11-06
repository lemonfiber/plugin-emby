use super::{a_device, a_key, an_account, an_item, sourced, Account, Item};

#[test]
fn an_account_is_thirty_two_hex_digits_with_or_without_its_dashes() {
    assert!(an_account("4f53cda18c2baa0c0354bb5f9a3ecbe5"));
    assert!(an_account("4f53cda1-8c2b-aa0c-0354-bb5f9a3ecbe5"));
    for refused in [
        "",
        "4f53cda18c2baa0c0354bb5f9a3ecbe",
        "4f53cda18c2baa0c0354bb5f9a3ecbe5a",
        "4f53cda1-8c2baa0c-0354-bb5f9a3ecbe5",
        "../Users/4f53cda18c2baa0c0354bb5f9",
        "4f53cda18c2baa0c0354bb5f9a3ecbeg",
        "4f53cda18c2baa0c0354bb5f9a3ecb?5",
    ] {
        assert!(!an_account(refused), "{refused:?} was taken for an account");
    }
}

#[test]
fn an_item_is_one_to_nineteen_digits() {
    assert!(an_item("8"));
    assert!(an_item(&"9".repeat(19)));
    for refused in [
        "",
        "8a",
        "-8",
        "../8",
        &"9".repeat(20),
        "4f53cda18c2baa0c0354bb5f9a3ecbe5",
    ] {
        assert!(!an_item(refused), "{refused:?} was taken for an item");
    }
}

#[test]
fn a_media_source_names_the_item_after_its_prefix_or_alone() {
    assert_eq!(sourced("mediasource_8"), Some("8"));
    assert_eq!(sourced("MediaSource_8"), Some("8"));
    assert_eq!(sourced("8"), Some("8"));
    for refused in [
        "mediasource_",
        "mediasource_x",
        "source_8",
        "mediasource_8/9",
        "",
    ] {
        assert_eq!(sourced(refused), None, "{refused:?} named an item");
    }
}

#[test]
fn a_held_value_is_written_as_it_came() {
    assert_eq!(
        Account::of("4f53cda18c2baa0c0354bb5f9a3ecbe5").map(|id| id.to_string()),
        Some("4f53cda18c2baa0c0354bb5f9a3ecbe5".to_owned())
    );
    assert_eq!(Account::of("8"), None);
    assert_eq!(Item::of("8").map(|id| id.to_string()), Some("8".to_owned()));
    assert_eq!(Item::of("x"), None);
}

#[test]
fn a_device_is_eight_to_sixty_four_letters_digits_and_dashes_not_all_digits() {
    assert!(a_device("phone-0001"));
    assert!(a_device(&"a".repeat(64)));
    for refused in [
        "short",
        "",
        "has a space",
        "quote\"close",
        &"a".repeat(65),
        "12345678",
    ] {
        assert!(!a_device(refused), "{refused:?} was taken for a device");
    }
}

#[test]
fn a_key_is_letters_and_digits_and_never_a_path() {
    assert!(a_key("0f3a9c2b"));
    for refused in ["", "a/b", "a?b=c", "../keys", &"a".repeat(129)] {
        assert!(!a_key(refused), "{refused:?} was taken for a key");
    }
}
