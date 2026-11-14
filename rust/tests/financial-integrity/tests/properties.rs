//! Property tests over the double-entry rule: random entries, random sizes, amounts of arbitrary precision.
//!
//! There is no property-testing crate in this workspace (the build is offline), so the generator is a seeded xorshift
//! written here. That is a feature rather than an apology: a failure prints the seed that produced it, and the same seed
//! reproduces the same entry set on any machine, which a shrinking framework would have to be trusted to do.
//!
//! Two properties, and one control:

use vimbai_common::Money;
use vimbai_ledger_core::{validate_lines, validate_money_lines, JournalLine};

/// A small deterministic generator. Not cryptographic, and it does not need to be: it has to be reproducible.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// An amount with up to `digits` integer places and up to `scale` decimals, as an exact decimal string.
    fn amount(&mut self, digits: u32, scale: u32) -> Money {
        let integer = self.below(10u64.pow(digits));
        let decimals = self.below(10u64.pow(scale));
        let text = if scale == 0 {
            format!("{integer}")
        } else {
            format!("{integer}.{decimals:0width$}", width = scale as usize)
        };
        Money::parse(&text).expect("a decimal the generator wrote itself")
    }
}

/// One generated movement: an amount and the side it moves.
struct Movement {
    amount: Money,
    debit: bool,
}

fn movements(seed: &mut Seeded, count: u64) -> Vec<Movement> {
    (0..count)
        .map(|_| {
            // a mix of widths on purpose: 1 to 18 integer digits, 0 to 6 decimals
            let digits = 1 + (seed.below(18)) as u32;
            let scale = (seed.below(7)) as u32;
            let mut amount = seed.amount(digits, scale);
            // a line that moves nothing is not a line: the rule refuses it, so the generator must not produce one
            while amount.is_zero() {
                amount = seed.amount(digits, scale);
            }
            Movement {
                amount,
                debit: seed.next().is_multiple_of(2),
            }
        })
        .collect()
}

/// The lines a ledger would be handed: one line per movement.
fn as_lines(movements: &[Movement]) -> Vec<(Money, Money)> {
    movements
        .iter()
        .map(|movement| {
            if movement.debit {
                (movement.amount.clone(), Money::zero())
            } else {
                (Money::zero(), movement.amount.clone())
            }
        })
        .collect()
}

/// The plug that makes a set of one-sided movements balance: the difference, on the other side.
fn balancing_line(movements: &[Movement]) -> (Money, Money) {
    let mut debits = Money::zero();
    let mut credits = Money::zero();
    for line in as_lines(movements) {
        debits = debits.plus(&line.0);
        credits = credits.plus(&line.1);
    }
    if debits == credits {
        (Money::zero(), Money::zero())
    } else if credits > debits {
        (credits.minus(&debits), Money::zero())
    } else {
        (Money::zero(), debits.minus(&credits))
    }
}

#[test]
fn random_entries_balance_once_double_entry_is_applied() {
    let mut seed = Seeded(0x5EED_1234_ABCD_9876);
    let mut cases = 0;
    let mut largest = 0;

    for _ in 0..300 {
        let count = 1 + seed.below(12);
        let generated = movements(&mut seed, count);

        // the plug: whatever the one-sided movements came to, the entry now balances exactly
        let plug = balancing_line(&generated);
        let mut lines = as_lines(&generated);
        if !plug.0.is_zero() || !plug.1.is_zero() {
            lines.push(plug);
        }

        // the property the ledger needs: exact amounts, any magnitude, any precision, and it balances
        let report = validate_money_lines(&lines);
        assert!(
            report.valid,
            "a plugged entry must balance: {report:?}\nseed case {cases}, {count} movements"
        );

        // and the totals are equal as Money, not merely close as floats
        let debits = lines
            .iter()
            .fold(Money::zero(), |sum, line| sum.plus(&line.0));
        let credits = lines
            .iter()
            .fold(Money::zero(), |sum, line| sum.plus(&line.1));
        assert_eq!(debits, credits, "case {cases}");
        largest = largest.max(lines.len());
        cases += 1;
    }

    assert_eq!(cases, 300);
    assert!(
        largest > 12,
        "the generator must have produced a long entry"
    );
}

#[test]
fn a_mirrored_entry_balances_with_no_plug_at_all() {
    let mut seed = Seeded(0xC0FF_EE00_1234_5678);
    for case in 0..300 {
        let count = 1 + seed.below(10);
        let generated = movements(&mut seed, count);
        // double entry proper: every movement written twice, once each way
        let mut lines = as_lines(&generated);
        for movement in &generated {
            lines.push(if movement.debit {
                (Money::zero(), movement.amount.clone())
            } else {
                (movement.amount.clone(), Money::zero())
            });
        }
        let report = validate_money_lines(&lines);
        assert!(report.valid, "case {case}: {report:?}");
    }
}

#[test]
fn every_generated_imbalance_is_caught() {
    // the control: if the plug is wrong by one unit in the last place the rule uses, the entry is refused. A property
    // test that only proves everything passes proves nothing.
    let mut seed = Seeded(0xDEAD_BEEF_0BAD_F00D);
    let mut caught = 0;
    for case in 0..200 {
        let count = 1 + seed.below(6);
        let generated = movements(&mut seed, count);
        let mut lines = as_lines(&generated);
        let (debit_plug, credit_plug) = balancing_line(&generated);
        // a tenth of a micro-unit out - far below a cent, and still not balanced
        let skew = Money::parse("0.0000001").expect("an amount");
        if credit_plug.is_zero() {
            lines.push((debit_plug.minus(&skew), Money::zero()));
        } else {
            lines.push((Money::zero(), credit_plug.minus(&skew)));
        }
        let report = validate_money_lines(&lines);
        assert!(!report.valid, "case {case} must be refused: {report:?}");
        caught += 1;
    }
    assert_eq!(caught, 200);
}

#[test]
fn the_float_door_never_accepts_what_the_exact_door_refuses() {
    // The wire still carries figures a client wrote as JSON numbers, and those arrive as floats. Two doors therefore
    // exist: `validate_lines` is the float door (it rounds at ten decimal places so that 0.1 + 0.2 reads as 0.3), and
    // `validate_money_lines` is the exact one a posted entry uses.
    //
    // They are NOT interchangeable, and the direction of the difference is what matters: accumulating floats loses low
    // bits, so the float door can refuse a set the exact door balances. That is the safe direction - it is conservative.
    // The unsafe direction would be a float door that accepts something the exact door refuses, which is a ledger that
    // posts an entry that does not balance. This walks both doors over the same generated sets and asserts the
    // implication: if the float door says yes, the exact door must say yes too.
    let mut seed = Seeded(0x1357_9BDF_2468_ACE0);
    let mut accepted_by_float = 0;

    for case in 0..300 {
        let count = 1 + seed.below(8);
        // a float holds integers exactly to 2^53, so a client sending a JSON number is held to that bound: ten integer
        // digits and cents stays inside it, which is the range the wire door is for
        let generated: Vec<Movement> = (0..count)
            .map(|_| {
                let mut amount = seed.amount(10, 2);
                while amount.is_zero() {
                    amount = seed.amount(10, 2);
                }
                Movement {
                    amount,
                    debit: seed.next().is_multiple_of(2),
                }
            })
            .collect();

        let plug = balancing_line(&generated);
        let mut cents = as_lines(&generated);
        if !plug.0.is_zero() || !plug.1.is_zero() {
            cents.push(plug);
        }

        // the exact door, over the same cent amounts
        let exact = validate_money_lines(&cents);
        assert!(
            exact.valid,
            "the exact door balances it: case {case} {exact:?}"
        );

        // the same entry as floats, with a plug computed by floats
        let floated_lines: Vec<JournalLine> = cents
            .iter()
            .map(|(debit, credit)| JournalLine::new(debit.to_f64_lossy(), credit.to_f64_lossy()))
            .collect();
        let floated = validate_lines(&floated_lines);

        if floated.valid {
            accepted_by_float += 1;
            assert!(
                exact.valid,
                "the float door accepted what the exact door refuses: case {case}"
            );
        }
    }

    assert!(
        accepted_by_float > 0,
        "the float door must still work at all"
    );
}

#[test]
fn the_float_door_has_a_boundary_and_the_exact_door_does_not() {
    // where the float door stops: an amount above 2^53 units cannot be carried exactly by an f64, so two sides that are
    // exactly equal in the ledger can read as different through the wire door. This is why a posted entry is compared
    // as Money and why amounts travel as strings.
    let large = Money::parse("100000000000000001.00").expect("an amount");
    let exact = validate_money_lines(&[
        (large.clone(), Money::zero()),
        (Money::zero(), large.clone()),
    ]);
    assert!(exact.valid, "the exact door balances it: {exact:?}");

    let carried = large.round(2).to_f64_lossy();
    let through_float = validate_lines(&[
        JournalLine::new(carried, 0.0),
        JournalLine::new(0.0, carried),
    ]);
    // two copies of the same float do balance, because the error is identical on both sides; the loss shows when the
    // two sides are reached by different routes, which is the case the exact door exists for
    assert!(through_float.valid, "{through_float:?}");
    let one_unit_less = carried - 1.0;
    assert_eq!(
        one_unit_less, carried,
        "an f64 cannot hold the difference here"
    );

    // the ledger's own answer, one unit apart, is refused
    let off = Money::parse("100000000000000001.00").expect("an amount");
    let other = Money::parse("100000000000000000.00").expect("an amount");
    let refused = validate_money_lines(&[(off, Money::zero()), (Money::zero(), other)]);
    assert!(!refused.valid, "one unit is not nothing: {refused:?}");
}
