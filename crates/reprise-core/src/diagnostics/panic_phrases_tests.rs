//! Every phrase of the table against the real panic of the running toolchain.

use std::cell::RefCell;
use std::hint::black_box;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use super::crash_reduce::{reduce_message, PanicPayload};
use super::log_template::LogTemplate;
use super::log_templates::best_match;
use super::panic_phrases::{phrase_sources, phrases};

/// A phrase's real panic and what its message reduces to.
struct Case {
    reduced: &'static str,
    trigger: fn(),
    /// Arithmetic overflow only panics in a build with overflow checks.
    needs_overflow_checks: bool,
}

const fn case(reduced: &'static str, trigger: fn()) -> Case {
    Case {
        reduced,
        trigger,
        needs_overflow_checks: false,
    }
}

const fn overflow_case(reduced: &'static str, trigger: fn()) -> Case {
    Case {
        reduced,
        trigger,
        needs_overflow_checks: true,
    }
}

fn captured(trigger: fn()) -> PanicPayload {
    let error = catch_unwind(AssertUnwindSafe(trigger)).expect_err("the trigger must panic");
    PanicPayload::of(error.as_ref())
}

fn text_of(payload: &PanicPayload) -> String {
    match payload {
        PanicPayload::Static(text) => (*text).to_owned(),
        PanicPayload::Formatted(text) => text.clone(),
        PanicPayload::Other => String::new(),
    }
}

fn cases() -> Vec<Case> {
    vec![
        case("called `Option::unwrap()` on a `None` value", || {
            black_box(None::<u8>).unwrap();
        }),
        case("called `Result::unwrap()` on an `Err` value: …", || {
            black_box(Err::<u8, _>("Zqx Err")).unwrap();
        }),
        case(
            "index out of bounds: the len is … but the index is …",
            || {
                let _ = black_box(vec![1, 2, 3])[black_box(7)];
            },
        ),
        case(
            "range start index … out of range for slice of length …",
            || {
                let _ = &black_box(vec![1, 2, 3])[black_box(5)..];
            },
        ),
        case(
            "range end index … out of range for slice of length …",
            || {
                let _ = &black_box(vec![1, 2, 3])[..black_box(5)];
            },
        ),
        case("slice index starts at … but ends at …", || {
            let _ = &black_box(vec![1, 2, 3])[black_box(3)..black_box(1)];
        }),
        case(
            "start byte index … is out of bounds for string of length …",
            || {
                let _ = &black_box(String::from("Zqx"))[black_box(100)..];
            },
        ),
        case(
            "end byte index … is out of bounds for string of length …",
            || {
                let _ = &black_box(String::from("Zqx"))[..black_box(100)];
            },
        ),
        case(
            "start byte index … is not a char boundary; it is inside … (bytes … of string)",
            || {
                let _ = &black_box(String::from("Zqxé"))[black_box(4)..];
            },
        ),
        case(
            "end byte index … is not a char boundary; it is inside … (bytes … of string)",
            || {
                let _ = &black_box(String::from("Zqxé"))[..black_box(4)];
            },
        ),
        case("byte range starts at … but ends at …", || {
            let _ = &black_box(String::from("Zqx"))[black_box(3)..black_box(1)];
        }),
        overflow_case("attempt to add with overflow", || {
            let _ = black_box(i32::MAX) + black_box(1);
        }),
        overflow_case("attempt to subtract with overflow", || {
            let _ = black_box(i32::MIN) - black_box(1);
        }),
        overflow_case("attempt to multiply with overflow", || {
            let _ = black_box(i32::MAX) * black_box(2);
        }),
        case("attempt to divide by zero", || {
            let _ = black_box(1) / black_box(0);
        }),
        case("attempt to divide with overflow", || {
            let _ = black_box(i32::MIN) / black_box(-1);
        }),
        case(
            "attempt to calculate the remainder with a divisor of zero",
            || {
                let _ = black_box(1) % black_box(0);
            },
        ),
        case("attempt to calculate the remainder with overflow", || {
            let _ = black_box(i32::MIN) % black_box(-1);
        }),
        overflow_case("attempt to negate with overflow", || {
            let _ = -black_box(i32::MIN);
        }),
        overflow_case("attempt to shift left with overflow", || {
            let _ = black_box(1u32) << black_box(40u32);
        }),
        overflow_case("attempt to shift right with overflow", || {
            let _ = black_box(1u32) >> black_box(40u32);
        }),
        case("RefCell already borrowed", || {
            let cell = RefCell::new(1);
            let _first = cell.borrow_mut();
            let _second = cell.borrow_mut();
        }),
        case("RefCell already mutably borrowed", || {
            let cell = RefCell::new(1);
            let _first = cell.borrow_mut();
            let _second = cell.borrow();
        }),
        case(
            "assertion `left == right` failed\n  left: …\n right: …",
            || {
                assert_eq!(black_box(1), black_box(2));
            },
        ),
        case(
            "assertion `left == right` failed: …\n  left: …\n right: …",
            || {
                assert_eq!(black_box(1), black_box(2), "Zqx {}", black_box(5));
            },
        ),
        case(
            "assertion `left != right` failed\n  left: …\n right: …",
            || {
                assert_ne!(black_box(1), black_box(1));
            },
        ),
        case(
            "assertion `left != right` failed: …\n  left: …\n right: …",
            || {
                assert_ne!(black_box(1), black_box(1), "Zqx {}", black_box(5));
            },
        ),
        case("internal error: entered unreachable code: …", || {
            unreachable!("Zqx {}", black_box(1));
        }),
        case("not yet implemented: …", || {
            todo!("Zqx {}", black_box(1));
        }),
        case("not implemented: …", || {
            unimplemented!("Zqx {}", black_box(1));
        }),
        case("removal index (is …) should be < len (is …)", || {
            black_box(vec![1]).remove(black_box(5));
        }),
        case("insertion index (is …) should be <= len (is …)", || {
            black_box(vec![1]).insert(black_box(5), 1);
        }),
        case(
            "swap_remove index (is …) should be < len (is …)",
            || {
                black_box(vec![1]).swap_remove(black_box(5));
            },
        ),
        case(
            "`at` split index (is …) should be <= len (is …)",
            || {
                let _ = black_box(vec![1, 2, 3]).split_off(black_box(9));
            },
        ),
        case(
            "copy_from_slice: source slice length (…) does not match destination slice length (…)",
            || {
                let mut target = [0u8; 2];
                target.copy_from_slice(&black_box([1u8, 2, 3])[..]);
            },
        ),
        case("overflow when adding durations", || {
            let _ = black_box(Duration::MAX) + black_box(Duration::from_secs(1));
        }),
        case("overflow when subtracting durations", || {
            let _ = black_box(Duration::ZERO) - black_box(Duration::from_secs(1));
        }),
        case("window size must be non-zero", || {
            let _ = black_box(vec![1u8]).windows(black_box(0)).count();
        }),
    ]
}

fn escaped(text: &str) -> String {
    text.replace('\n', "\\n")
}

#[test]
fn every_phrase_matches_the_message_of_its_real_panic() {
    for case in cases() {
        if case.needs_overflow_checks && !cfg!(debug_assertions) {
            continue;
        }
        let payload = captured(case.trigger);
        let text = text_of(&payload);

        let found = best_match(phrases(), &text).map(LogTemplate::reduced);

        assert_eq!(
            found.as_deref(),
            Some(case.reduced),
            "the toolchain's panic text {text:?} no longer matches its phrase"
        );
    }
}

#[test]
fn the_hook_shows_each_real_panic_as_its_phrase_and_nothing_of_the_values() {
    for case in cases() {
        if case.needs_overflow_checks && !cfg!(debug_assertions) {
            continue;
        }
        let payload = captured(case.trigger);

        let shown = reduce_message(&payload, None);

        assert_eq!(shown, escaped(case.reduced), "{:?}", text_of(&payload));
        assert!(!shown.to_lowercase().contains("zqx"), "{shown}");
    }
}

#[test]
fn the_table_holds_exactly_the_phrases_a_real_panic_is_tested_for() {
    let tested: Vec<&str> = cases().iter().map(|case| case.reduced).collect();
    let table: Vec<String> = phrases().iter().map(LogTemplate::reduced).collect();

    assert_eq!(
        phrases().len(),
        phrase_sources().len(),
        "a phrase does not parse"
    );
    for reduced in &table {
        assert!(
            tested.contains(&reduced.as_str()),
            "{reduced:?} has no real-panic test"
        );
    }
    for reduced in tested {
        assert!(
            table.iter().any(|phrase| phrase == reduced),
            "{reduced:?} is tested but not in the table"
        );
    }
}
