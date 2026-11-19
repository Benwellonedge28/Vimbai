#![forbid(unsafe_code)]
//! Exchange differences, as IAS 21 states them, answered once for the whole system.
//!
//! A payment app asks what one money is worth in another today. An accounting system asks a harder question: what was
//! this worth when it was invoiced, when it was paid, and when the books were closed, and what is the difference. That
//! difference is real money - it is why a Harare business can show a profit and have less cash - so it is worked out in
//! one place, in exact decimals, and never guessed at by a feature.
//!
//! Four rules hold here:
//!
//! * a difference is a figure with a sign, never a loss folded into a gain;
//! * which rate an item is restated at depends on what the item *is*, not on what is convenient: money owed and money
//!   held are restated at the closing rate, income and expenses are translated at the average, and what the owners put in
//!   stays at the rate of the day it went in;
//! * a thing bought is not money: stock and machines are carried at the rate of the day they were bought, so a
//!   devaluation never invents a profit on them;
//! * translating a set of books never quietly loses the leftover. Whatever closing, average and historical rates leave
//!   behind is the cumulative translation adjustment, and it is reported rather than absorbed.

use crate::currencies::converted;
use crate::money::Money;

/// What a balance or a flow is, which is what decides the rate it is restated at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// Money held or money owed: cash, a bank balance, a debtor, a creditor, a loan. Restated at the closing rate.
    Monetary,
    /// A thing bought rather than money: stock, a machine, a building, an intangible. Kept at its own day's rate.
    NonMonetary,
    /// Sales and other income, translated at the average rate for the period.
    Income,
    /// Costs, translated at the average rate for the period.
    Expense,
    /// What the owners put in and what has been kept in the business, at the rate of the day it arose.
    Equity,
}

/// The three rates a period is translated at, each stated in the money being translated into.
#[derive(Debug, Clone)]
pub struct Rates {
    /// The rate on the last day of the period, for money held and money owed.
    pub closing: Money,
    /// The average rate over the period, for income and costs.
    pub average: Money,
    /// The rate of the day the item arose, for what the owners put in and for things bought.
    pub historical: Money,
}

impl Rates {
    /// The rate this kind of item is restated at.
    pub fn for_item(&self, item: Item) -> &Money {
        match item {
            Item::Monetary => &self.closing,
            Item::Income | Item::Expense => &self.average,
            Item::NonMonetary | Item::Equity => &self.historical,
        }
    }
}

/// The difference between what a balance was carried at and what it is worth now, keeping its sign.
///
/// Positive is a gain and negative is a loss, so a caller never has to know which way the money moved. The same helper
/// answers both differences an accounting system needs: the realised one when a debt is settled, and the unrealised one
/// when a balance is restated at a later rate.
pub fn difference(carried: &Money, restated: &Money) -> Money {
    restated.minus(carried)
}

/// An amount restated into the money the books are kept in, at the rate the item's kind calls for.
pub fn restated(
    amount: &Money,
    from: &str,
    to: &str,
    item: Item,
    rates: &Rates,
) -> Result<Money, String> {
    converted(from, to, amount, rates.for_item(item))
}

/// A set of books translated into the money it is being presented in, before the leftover is placed.
#[derive(Debug, Clone, Default)]
pub struct Translated {
    /// Everything owned, at the closing rate.
    pub assets: Money,
    /// Everything owed, at the closing rate, as a positive figure.
    pub liabilities: Money,
    /// All income for the period, at the average rate.
    pub income: Money,
    /// All costs for the period, at the average rate, as a positive figure.
    pub expenses: Money,
    /// What the owners put in and what was kept, at the rates of the days they arose.
    pub equity: Money,
}

impl Translated {
    /// The cumulative translation adjustment: what the three rates leave over, in the money presented in.
    ///
    /// This is the figure that goes to equity on consolidation. Reporting it is what stops a group's balance sheet from
    /// balancing only because a rounding was quietly hidden somewhere.
    pub fn cumulative_translation_adjustment(&self) -> Money {
        let worth = self.assets.minus(&self.liabilities);
        let funded = self.equity.plus(&self.income).minus(&self.expenses);
        worth.minus(&funded)
    }

    /// Whether the translation adds up: the worth of the business equals what funded it, once the adjustment is placed.
    pub fn balances(&self) -> bool {
        self.assets
            .minus(&self.liabilities)
            .minus(&self.equity.plus(&self.income).minus(&self.expenses))
            .minus(&self.cumulative_translation_adjustment())
            .is_zero()
    }
}

/// The gap between the rate a return must be filed at and the rate the market is actually using, when it is wide enough
/// to change the story.
///
/// The official rate is what posts, so the return matches what the accountant files. The market rate is shown beside it,
/// because a business whose official books show a profit and whose market books show a loss needs to be told, not
/// comforted. Nothing here decides which rate is right: it reports the distance between them.
pub fn gap(official: &Money, market: &Money) -> Option<Money> {
    if official.is_zero() || market.is_zero() {
        return None;
    }
    let difference = market.minus(official).abs();
    let hundred = Money::from_i64(100);
    let percent = difference.times(&hundred).over(official);
    if percent.as_decimal() < &Money::parse("5").expect("five").as_decimal().clone() {
        return None;
    }
    Some(percent.round(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("a figure")
    }

    fn rates() -> Rates {
        Rates {
            closing: money("40"),
            average: money("38"),
            historical: money("36"),
        }
    }

    #[test]
    fn money_owed_is_restated_at_the_closing_rate_and_things_bought_are_not() {
        let rates = rates();
        let owed =
            restated(&money("500"), "GBP", "ZWG", Item::Monetary, &rates).expect("a stated rate");
        assert_eq!(
            owed.to_fixed(2),
            "20000.00",
            "500 at the closing rate of 40"
        );

        let machine = restated(&money("50000"), "USD", "ZWG", Item::NonMonetary, &rates)
            .expect("a stated rate");
        assert_eq!(
            machine.to_fixed(2),
            "1800000.00",
            "a machine stays at the rate of the day it was bought: a devaluation invents no profit on it"
        );

        let sales =
            restated(&money("1000"), "GBP", "ZWG", Item::Income, &rates).expect("a stated rate");
        assert_eq!(
            sales.to_fixed(2),
            "38000.00",
            "income is translated at the average"
        );
    }

    #[test]
    fn settling_a_debt_at_a_later_rate_leaves_a_gain_or_a_loss_with_its_sign() {
        let carried = money("36000"); // 1,000 GBP invoiced at 36
        let settled = money("38000"); // paid at 38
        assert_eq!(
            difference(&carried, &settled).to_fixed(2),
            "2000.00",
            "a gain"
        );

        let weaker = money("34000");
        assert_eq!(
            difference(&carried, &weaker).to_fixed(2),
            "-2000.00",
            "a loss keeps its sign"
        );
    }

    #[test]
    fn the_leftover_of_translation_is_reported_as_the_cumulative_adjustment() {
        // assets 200,000 - liabilities 120,000 = 80,000 of worth; funded by 60,000 equity, 40,000 income, 18,000 costs
        let books = Translated {
            assets: money("200000"),
            liabilities: money("120000"),
            income: money("40000"),
            expenses: money("18000"),
            equity: money("60000"),
        };
        // worth 80,000 against funded 82,000 leaves an adjustment of -2,000, which is placed in equity
        assert_eq!(
            books.cumulative_translation_adjustment().to_fixed(2),
            "-2000.00"
        );
        assert!(
            books.balances(),
            "the adjustment is what makes the translation add up"
        );
    }

    #[test]
    fn a_rate_gap_is_reported_only_when_it_is_wide_enough_to_matter() {
        assert!(
            gap(&money("36"), &money("36.5")).is_none(),
            "a small gap is noise"
        );
        let wide = gap(&money("36"), &money("45")).expect("a quarter wider is a warning");
        assert_eq!(wide.to_fixed(2), "25.00");
    }
}
