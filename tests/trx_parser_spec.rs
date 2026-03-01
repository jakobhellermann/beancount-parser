#![allow(clippy::items_after_test_module, clippy::pedantic, missing_docs)]

use std::collections::HashSet;

use rstest::rstest;

use beancount_parser::{
    metadata, parse, parse_iter, Directive, DirectiveContent, Entry, Posting, PostingPrice,
    Transaction,
};

const COMMENTS: &str = include_str!("samples/comments.beancount");
const SIMPLE: &str = include_str!("samples/simple.beancount");
const OFFICIAL: &str = include_str!("samples/official.beancount");

#[rstest]
#[case("", 0)]
#[case(COMMENTS, 0)]
#[case(SIMPLE, 4)]
#[case(OFFICIAL, 865)]
fn should_find_all_transactions(#[case] input: &str, #[case] expected_count: usize) {
    let actual_count = parse_iter::<f64>(input)
        .map(|res| res.expect("parsing should succeed"))
        .filter_map(|entry| match entry {
            Entry::Directive(Directive {
                content: DirectiveContent::Transaction(trx),
                ..
            }) => Some(trx),
            _ => None,
        })
        .count();
    assert_eq!(actual_count, expected_count);
}

#[rstest]
#[case("", 0)]
#[case(COMMENTS, 0)]
#[case(SIMPLE, 15)]
#[case(OFFICIAL, 2665)]
fn should_find_all_postings(#[case] input: &str, #[case] expected_count: usize) {
    let actual_count: usize = parse::<f64>(input)
        .expect("parsing should succeed")
        .directives
        .into_iter()
        .map(|d| match d.content {
            DirectiveContent::Transaction(trx) => trx.postings.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(actual_count, expected_count);
}

#[rstest]
#[case("2023-05-15 txn", None)]
#[case("2023-05-15 txn \"Hello world!\"", Some("Hello world!"))]
#[case::escaped_double_quotes("2023-05-15 txn \"Hello \\\"world\\\"!\"", Some("Hello \"world\"!"))]
#[case::escaped_backslash("2023-05-15 txn \"Hello \\\\world!\"", Some("Hello \\world!"))]
#[case("2023-05-15 txn \"payee\" \"narration\"", Some("narration"))]
#[case(
    "2023-05-15 txn \"Hello world!\" ; And a comment",
    Some("Hello world!")
)]
fn should_parse_transaction_description(#[case] input: &str, #[case] expected: Option<&str>) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    assert_eq!(trx.narration.as_deref(), expected);
}

#[rstest]
#[case("2023-05-15 txn", None)]
#[case("2023-05-15 txn \"Hello world!\"", None)]
#[case("2023-05-15 txn \"payee\" \"narration\"", Some("payee"))]
#[case(
    "2023-05-15 txn \"Hello world!\" \"\"; And a comment",
    Some("Hello world!")
)]
fn should_parse_transaction_payee(#[case] input: &str, #[case] expected: Option<&str>) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    assert_eq!(trx.payee.as_deref(), expected);
}

#[rstest]
#[case("2023-05-15 txn \"Hello world!\"", &[])]
#[case("2023-05-15 txn \"Hello world!\" ^a", &["a"])]
#[case("2023-05-15 txn \"Hello world!\" ^a.b", &["a.b"])]
#[case("2023-05-15 txn \"Hello world!\" #a", &[])]
#[case("2023-05-15 txn \"Hello world!\" ^link-a ^link-b", &["link-a", "link-b"])]
#[case("2023-05-15 txn \"Hello world!\" ^link-a #tag ^link-b", &["link-a", "link-b"])]
#[case("2023-05-15 txn \"Hello world!\"^link-a#tag^link-b", &["link-a", "link-b"])]
fn should_parse_transaction_links(#[case] input: &str, #[case] expected: &[&str]) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    assert_eq!(
        trx.links.iter().map(AsRef::as_ref).collect::<HashSet<_>>(),
        expected.iter().copied().collect::<HashSet<_>>()
    );
}

#[rstest]
#[case("2023-05-15 txn \"Hello world!\"", &[])]
#[case("2023-05-15 txn \"Hello world!\" ^a", &[])]
#[case("2023-05-15 txn \"Hello world!\" #a", &["a"])]
#[case("2023-05-15 txn \"Hello world!\" #tag-a #tag-b", &["tag-a", "tag-b"])]
#[case("2023-05-15 txn \"Hello world!\" #tag-a ^link #tag-b", &["tag-a", "tag-b"])]
fn should_parse_transaction_tags(#[case] input: &str, #[case] expected: &[&str]) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    assert_eq!(
        trx.tags.iter().map(AsRef::as_ref).collect::<HashSet<_>>(),
        expected.iter().copied().collect::<HashSet<_>>()
    );
}

#[rstest]
#[case("2023-05-15 txn", None)]
#[case("2023-05-15 txn \"hello\"", None)]
#[case("2023-05-15 *", Some('*'))]
#[case("2023-05-15 * \"hello\"", Some('*'))]
#[case("2023-05-15 !", Some('!'))]
#[case("2023-05-15 ! \"hello\"", Some('!'))]
#[case("2023-05-15 ? \"hello\"", Some('?'))]
#[case("2023-05-15 P \"hello\"", Some('P'))]
fn should_parse_transaction_flag(#[case] input: &str, #[case] expected: Option<char>) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    assert_eq!(trx.flag, expected);
}

#[rstest]
#[case("2014-04-23 * \"Flight to Berlin\"\n  Expenses:Flights -1230.27 USD\n  Liabilities:CreditCard", &[])]
#[case("2014-04-23 * \"Flight to Berlin\" #berlin-trip-2014\n  Expenses:Flights -1230.27 USD\n  Liabilities:CreditCard", &["berlin-trip-2014"])]
#[case("2014-04-23 * #hello-world #2023_05", &["hello-world", "2023_05"])]
#[case("2014-04-23 * \"without-space\"#hello-world#2023_05", &["hello-world", "2023_05"])]
fn should_parse_tags(#[case] input: &str, #[case] expected: &[&str]) {
    let expected: HashSet<_> = expected.iter().copied().collect();
    let trx = parse_single_transaction(input);
    assert_eq!(
        trx.tags.iter().map(AsRef::as_ref).collect::<HashSet<_>>(),
        expected
    );
}

#[rstest]
#[case("2023-05-15 txn", &[])]
#[case("2023-05-15 txn\n  Assets:Cash", &["Assets:Cash"])]
#[case("2023-05-15 * \"Hello\" ; with comment \n  Assets:Cash", &["Assets:Cash"])]
#[case("2023-05-15 txn\n  Assets:Cash\n Income:Salary", &["Assets:Cash", "Income:Salary"])]
#[case("2023-05-15 txn\n  Assets:Cash\n  ; A comment\n  Income:Salary", &["Assets:Cash", "Income:Salary"])]
#[case("2023-05-15 txn\n  Assets:Cash\n\n  Income:Salary", &["Assets:Cash", "Income:Salary"])]
fn should_parse_posting_accounts(#[case] input: &str, #[case] expected: &[&str]) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    let posting_accounts: Vec<&str> = trx.postings.iter().map(|p| p.account.as_str()).collect();
    assert_eq!(&posting_accounts, expected);
}

#[rstest]
#[case("2023-05-15 txn", &[])]
#[case("2023-05-15 txn\n  Assets:Cash", &[None])]
#[case("2023-05-15 * \"Hello\" ; with comment \n  Assets:Cash", &[None])]
#[case("2023-05-15 txn\n  * Assets:Cash\n  ! Income:Salary\n  Equity:Openings", &[Some('*'), Some('!'), None])]
#[case("2023-05-15 txn\n  P Assets:Cash\n  ? Income:Salary\n  Equity:Openings", &[Some('P'), Some('?'), None])]
fn should_parse_posting_flags(#[case] input: &str, #[case] expected: &[Option<char>]) {
    let DirectiveContent::Transaction(trx) = parse_single_directive(input).content else {
        panic!("was not a transaction");
    };
    let posting_accounts: Vec<Option<char>> = trx.postings.into_iter().map(|p| p.flag).collect();
    assert_eq!(&posting_accounts, expected);
}

#[rstest]
fn should_parse_posting_with_metadata() {
    let posting = parse_single_posting("2023-05-17 *\n  Assets:Cash\n    foo: \"bar\"");
    assert_eq!(
        posting.metadata.get("foo"),
        Some(&metadata::Value::String("bar".into()))
    );
}

#[rstest]
fn amount_should_be_empty_if_absent() {
    let posting = parse_single_posting("2023-05-17 *\n  Assets:Cash");
    assert!(posting.amount.is_none(), "{:?}", posting.amount);
}

#[rstest]
fn price_should_be_empty_if_absent(
    #[values("2023-05-17 *\n  Assets:Cash", "2023-05-17 *\n  Assets:Cash 10 CHF")] input: &str,
) {
    let posting = parse_single_posting(input);
    assert!(posting.price.is_none(), "{:?}", posting.price);
}

#[rstest]
#[case("10 CHF", 10.0, "CHF")]
#[case("1,000 CHF", 1_000.0, "CHF")]
#[case("0 USD", 0.0, "USD")]
#[case::neg("-1 EUR", -1.0, "EUR")]
#[case::neg_with_space("- 1 EUR", -1.0, "EUR")]
#[case::neg_priority("- 1 + 3 EUR", 2.0, "EUR")]
#[case::neg_group("-(1) EUR", -1.0, "EUR")]
#[case::neg_group_2("-(-2 + 1) EUR", 1.0, "EUR")]
#[case::neg_group_space("- (1) EUR", -1.0, "EUR")]
#[case::neg_group_space_2("- (- 2 + 1) EUR", 1.0, "EUR")]
#[case("1.2 PLN", 1.2, "PLN")]
#[case(".1 PLN", 0.1, "PLN")]
#[case("1. CHF", 1.0, "CHF")]
#[case("1 + 1 CHF", 1.0 + 1.0, "CHF")]
#[case("1 + 1 + 2 CHF", 1.0 + 1.0 + 2.0, "CHF")]
#[case("1+1 CHF", 1.0 + 1.0, "CHF")]
#[case("2 - 1 CHF", 2.0 - 1.0, "CHF")]
#[case("2 + 10 - 5 CHF", 2.0 + 10.0 - 5.0, "CHF")]
#[case("2+10-5 CHF", 2.0 + 10.0 - 5.0, "CHF")]
#[case("-2+10-5 CHF", -2.0 + 10.0 - 5.0, "CHF")]
#[case("10--2 CHF", 10.0 - -2.0, "CHF")]
#[case("2 + 10 + -5 CHF", 2.0 + 10.0 + -5.0, "CHF")]
#[case("2 * 3 CHF", 2.0 * 3.0, "CHF")]
#[case("2 * 3 + 4 CHF", 2.0 * 3.0 + 4.0, "CHF")]
#[case("2*3+4 CHF", 2.0 * 3.0 + 4.0, "CHF")]
#[case("2 + 3 * 4 CHF", 2.0 + 3.0 * 4.0, "CHF")]
#[case("(2 + 3) * 4 CHF", (2.0 + 3.0) * 4.0, "CHF")]
#[case("( 2 + 3 ) * 4 CHF", (2.0 + 3.0) * 4.0, "CHF")]
#[case("2 + (3 * 4) CHF", 2.0 + (3.0 * 4.0), "CHF")]
#[case("2 * 3 * 4 CHF", 2.0 * 3.0 * 4.0, "CHF")]
#[case("2*3*4 CHF", 2.0 * 3.0 * 4.0, "CHF")]
#[case("2 / 4 CHF", 2.0 / 4.0, "CHF")]
#[case("6 / 3 / 2 CHF", 6.0 / 3.0 / 2.0, "CHF")]
#[case("6/3/2 CHF", 6.0 / 3.0 / 2.0, "CHF")]
#[case("6 * 3 / 2 CHF", 6.0 * 3.0 / 2.0, "CHF")]
#[case("6 / 3 * 2 CHF", 6.0 / 3.0 * 2.0, "CHF")]
#[case("6 / 3 + 2 CHF", 6.0 / 3.0 + 2.0, "CHF")]
#[case("6 + 3 / 2 CHF", 6.0 + 3.0 / 2.0, "CHF")]
fn should_parse_amount(
    #[case] input: &str,
    #[case] expected_value: f64,
    #[case] expected_currency: &str,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash {input}");
    let amount = parse_single_posting(&input).amount.unwrap();
    assert_eq!(amount.value, expected_value);
    assert_eq!(amount.currency.as_str(), expected_currency);
}

#[rstest]
#[case("10 CHF", 10, "CHF")]
#[case("0 USD", 0, "USD")]
#[case("-1 EUR", -1, "EUR")]
#[case("1.2 PLN", 1.2, "PLN")]
#[case(".1 PLN", 0.1, "PLN")]
#[case("1. CHF", 1, "CHF")]
fn should_parse_unit_price(
    #[case] input: &str,
    #[case] expected_value: impl Into<f64>,
    #[case] expected_currency: &str,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash 1 DKK @ {input}");
    let PostingPrice::Unit(amount) = parse_single_posting(&input).price.unwrap() else {
        panic!("was not unit price");
    };
    assert_eq!(amount.value, expected_value.into());
    assert_eq!(amount.currency.as_str(), expected_currency);
}

#[rstest]
#[case("10 CHF", 10, "CHF")]
#[case("0 USD", 0, "USD")]
#[case("-1 EUR", -1, "EUR")]
#[case("1.2 PLN", 1.2, "PLN")]
#[case(".1 PLN", 0.1, "PLN")]
#[case("1. CHF", 1, "CHF")]
fn should_parse_total_price(
    #[case] input: &str,
    #[case] expected_value: impl Into<f64>,
    #[case] expected_currency: &str,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash 1 DKK @@ {input}");
    let PostingPrice::Total(amount) = parse_single_posting(&input).price.unwrap() else {
        panic!("was not unit price");
    };
    assert_eq!(amount.value, expected_value.into());
    assert_eq!(amount.currency.as_str(), expected_currency);
}

#[rstest]
fn cost_amount_should_be_empty_if_absent() {
    let input = "2023-05-19 *\n  Assets:Cash 10 CHF {}";
    let posting = parse_single_posting(input);
    let cost = posting.cost.unwrap().amount;
    assert!(cost.is_none(), "{cost:?}");
}

#[rstest]
fn cost_should_be_empty_if_absent(
    #[values(
        "2023-05-17 *\n  Assets:Cash",
        "2023-05-17 *\n  Assets:Cash 10 CHF",
        "2023-05-17 *\n  Assets:Cash 10 CHF @ 1 EUR"
    )]
    input: &str,
) {
    let posting = parse_single_posting(input);
    assert!(posting.cost.is_none(), "{:?}", posting.cost);
}

#[rstest]
#[case("Assets:Cash 1 CHF {1 EUR}", 1, "EUR")]
#[case("Assets:Cash 1 CHF { 1 EUR }", 1, "EUR")]
#[case("Assets:Cash 1 CHF {1 EUR} @ 4 PLN", 1, "EUR")]
fn should_parse_cost(
    #[case] input: &str,
    #[case] expected_value: impl Into<f64>,
    #[case] expected_currency: &str,
) {
    let input = format!("2023-05-17 *\n  {input}",);
    let amount = parse_single_posting(&input).cost.unwrap().amount.unwrap();
    assert_eq!(amount.value, expected_value.into());
    assert_eq!(amount.currency.as_str(), expected_currency);
}

#[rstest]
#[case("Assets:Cash 1 CHF {2023-05-19}", 2023, 5, 19)]
#[case("Assets:Cash 1 CHF {1 EUR, 2023-05-19}", 2023, 5, 19)]
#[case("Assets:Cash 1 CHF {1 EUR ,2023-05-19}", 2023, 5, 19)]
#[case("Assets:Cash 1 CHF {2023-05-19, 1 EUR}", 2023, 5, 19)]
fn should_parse_cost_date(
    #[case] input: &str,
    #[case] expected_year: u16,
    #[case] expected_month: u8,
    #[case] expected_day: u8,
) {
    let input = format!("2023-05-17 *\n  {input}",);
    let date = parse_single_posting(&input).cost.unwrap().date.unwrap();
    assert_eq!(date.year, expected_year);
    assert_eq!(date.month, expected_month);
    assert_eq!(date.day, expected_day);
}

#[rstest]
#[case("{}", None, None)]
#[case("{1 EUR}", Some(1.0), None)]
#[case("{ 1 EUR }", Some(1.0), None)]
#[case("{1 EUR} @ 4 PLN", Some(1.0), None)]
#[case("{{1 EUR}}", None, Some(1.0))]
#[case("{# 1 EUR}", None, Some(1.0))] // Space after # is required
#[case("{ 10.0 # 100.0 EUR}", Some(10.0), Some(100.0))]
fn should_parse_cost_total(
    #[case] input: &str,
    #[case] expected_value: Option<f64>,
    #[case] expected_total: Option<f64>,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash 1 CHF {input}",);
    let cost = parse_single_posting(&input).cost.unwrap();
    assert_eq!(cost.amount.map(|a| a.value), expected_value);
    assert_eq!(cost.total_amount.map(|a| a.value), expected_total);
}

#[rstest]
#[case::cost_with_date("Assets:Cash 10 CHF {350.00 EUR, 2026-01-15}", "2026-1-15")]
#[case::cost_with_date_reverse("Assets:Cash 10 CHF {2026-01-15, 350.00 EUR}", "2026-1-15")]
#[case::cost_with_date_no_space("Assets:Cash 10 CHF {350.00 EUR,2026-01-15}", "2026-1-15")]
#[case::cost_with_date_spaces("Assets:Cash 10 CHF { 350.00 EUR , 2026-01-15 }", "2026-1-15")]
#[case::total_cost_with_date("Assets:Cash 10 CHF {{ 350.00 EUR, 2026-01-15 }}", "2026-1-15")]
#[case::date_only("Assets:Cash 10 CHF {2026-01-15}", "2026-1-15")]
fn should_parse_cost_with_date(#[case] input: &str, #[case] expected_date: &str) {
    let input = format!("2026-01-15 *\n  {input}");
    let posting = parse_single_posting(&input);
    let cost = posting.cost.expect("cost should be present");
    let date = cost.date.expect("date should be present");
    let date = format!("{}-{}-{}", date.year, date.month, date.day);
    assert_eq!(date, expected_date);
}

#[rstest]
#[case(r#"{"lot-2026-01"}"#, "lot-2026-01", None, None)]
#[case(r#"{350.00 EUR, "lot-2026-01"}"#, "lot-2026-01", Some(350.00), None)]
#[case(r#"{"lot-2026-01", 350.00 EUR}"#, "lot-2026-01", Some(350.00), None)]
#[case(r#"{350.00 EUR, 2026-01-15, "lot-2026-01"}"#, "lot-2026-01", Some(350.00), Some((2026, 1, 15)))]
#[case(r#"{350.00 EUR, "lot-2026-01", 2026-01-15}"#, "lot-2026-01", Some(350.00), Some((2026, 1, 15)))]
#[case(r#"{2026-01-15, "lot-2026-01"}"#, "lot-2026-01", None, Some((2026, 1, 15)))]
#[case(r#"{"lot-2026-01", 2026-01-15}"#, "lot-2026-01", None, Some((2026, 1, 15)))]
fn should_parse_cost_with_label(
    #[case] input: &str,
    #[case] expected_label: &str,
    #[case] expected_amount: Option<f64>,
    #[case] expected_date: Option<(u16, u8, u8)>,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash 10 CHF {input}");
    let cost = parse_single_posting(&input).cost.unwrap();

    assert_eq!(
        cost.label.as_ref().map(|s| s.as_str()),
        Some(expected_label)
    );
    assert_eq!(cost.amount.as_ref().map(|a| a.value), expected_amount);
    assert_eq!(
        cost.date.as_ref().map(|d| (d.year, d.month, d.day)),
        expected_date
    );
    assert!(cost.total_amount.is_none());
    assert!(!cost.merge);
}

#[rstest]
#[case("{*}", None, None)]
#[case("{350.00 EUR, *}", Some(350.00), None)]
#[case("{*, 350.00 EUR}", Some(350.00), None)]
#[case("{350.00 EUR, 2026-01-15, *}", Some(350.00), Some((2026, 1, 15)))]
#[case("{# 3500.00 EUR, *}", None, None)]
fn should_parse_cost_with_merge_flag(
    #[case] input: &str,
    #[case] expected_amount: Option<f64>,
    #[case] expected_date: Option<(u16, u8, u8)>,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash 10 CHF {input}");
    let cost = parse_single_posting(&input).cost.unwrap();

    assert!(cost.merge);
    assert_eq!(cost.amount.as_ref().map(|a| a.value), expected_amount);
    assert_eq!(
        cost.date.as_ref().map(|d| (d.year, d.month, d.day)),
        expected_date
    );
    assert!(cost.label.is_none());
}

#[rstest]
#[case(r#"{350.00 # 3500.00 EUR, 2026-01-15, "lot-2026-01", *}"#, Some(350.00), Some(3500.00), Some((2026, 1, 15)), Some("lot-2026-01"), true)]
#[case(r#"{# 3500.00 EUR, 2026-01-15, "lot-2026-01", *}"#, None, Some(3500.00), Some((2026, 1, 15)), Some("lot-2026-01"), true)]
#[case(r#"{*, "label", 2026-01-15, 350.00 EUR}"#, Some(350.00), None, Some((2026, 1, 15)), Some("label"), true)]
fn should_parse_cost_with_all_features(
    #[case] input: &str,
    #[case] expected_per_unit: Option<f64>,
    #[case] expected_total: Option<f64>,
    #[case] expected_date: Option<(u16, u8, u8)>,
    #[case] expected_label: Option<&str>,
    #[case] expected_merge: bool,
) {
    let input = format!("2023-05-17 *\n  Assets:Cash 10 CHF {input}");
    let cost = parse_single_posting(&input).cost.unwrap();

    assert_eq!(cost.amount.as_ref().map(|a| a.value), expected_per_unit);
    assert_eq!(cost.total_amount.as_ref().map(|a| a.value), expected_total);
    assert_eq!(
        cost.date.as_ref().map(|d| (d.year, d.month, d.day)),
        expected_date
    );
    assert_eq!(cost.label.as_ref().map(|s| s.as_str()), expected_label);
    assert_eq!(cost.merge, expected_merge);
}

// ============================================================================
// EXHAUSTIVE PRICE SPECIFICATION TESTS
// Testing @ (per-unit) and @@ (total) price syntax
// ============================================================================

#[rstest]
#[case::unit_price_basic("Assets:Cash 10 CHF @ 350.00 EUR", true, 350.0, "EUR")]
#[case::unit_price_with_spaces("Assets:Cash 10 CHF  @  350.00 EUR", true, 350.0, "EUR")]
#[case::unit_price_decimal("Assets:Cash 10 CHF @ 197.90 USD", true, 197.90, "USD")]
#[case::unit_price_fractional("Assets:Cash 10 CHF @ 0.5 EUR", true, 0.5, "EUR")]
fn should_parse_unit_price_variations(
    #[case] input: &str,
    #[case] is_unit: bool,
    #[case] value: f64,
    #[case] currency: &str,
) {
    let input = format!("2026-01-15 *\n  {input}");
    let posting = parse_single_posting(&input);
    let price = posting.price.expect("price should be present");

    match price {
        PostingPrice::Unit(amount) => {
            assert!(is_unit, "expected unit price");
            assert_eq!(amount.value, value);
            assert_eq!(amount.currency.as_str(), currency);
        }
        PostingPrice::Total(_) => {
            assert!(!is_unit, "expected total price");
        }
    }
}

#[rstest]
#[case::total_price_basic("Assets:Cash 10 CHF @@ 3500.00 EUR", false, 3500.0, "EUR")]
#[case::total_price_with_spaces("Assets:Cash 10 CHF  @@  3500.00 EUR", false, 3500.0, "EUR")]
#[case::total_price_small("Assets:Cash 3 COM @@ 10.00 USD", false, 10.0, "USD")]
#[case::total_price_currency_conv("Assets:Bank:EUR -1000 EUR @@ 1100.00 USD", false, 1100.0, "USD")]
fn should_parse_total_price_variations(
    #[case] input: &str,
    #[case] is_unit: bool,
    #[case] value: f64,
    #[case] currency: &str,
) {
    let input = format!("2026-01-15 *\n  {input}");
    let posting = parse_single_posting(&input);
    let price = posting.price.expect("price should be present");

    match price {
        PostingPrice::Total(amount) => {
            assert!(!is_unit, "expected total price");
            assert_eq!(amount.value, value);
            assert_eq!(amount.currency.as_str(), currency);
        }
        PostingPrice::Unit(_) => {
            assert!(is_unit, "expected unit price");
        }
    }
}

#[rstest]
#[case::price_with_expression("Assets:Cash 10 CHF @ (350.00 + 5.00) EUR")]
#[case::price_with_calculation("Assets:Cash 10 CHF @ 350.00 * 1.1 EUR")]
fn should_parse_price_with_expressions(#[case] input: &str) {
    let input = format!("2026-01-15 *\n  {input}");
    let beancount = parse::<f64>(&input).expect("should parse price with expressions");
    let DirectiveContent::Transaction(trx) = &beancount.directives[0].content else {
        panic!("not a transaction");
    };
    let posting = &trx.postings[0];
    assert!(posting.price.is_some(), "price should be present");
}

// ============================================================================
// COST + PRICE COMBINATION TESTS
// Testing postings with both cost and price specifications
// ============================================================================

#[rstest]
#[case::cost_and_unit_price("Assets:Cash -10 HOOL {183.07 USD} @ 197.90 USD")]
#[case::cost_and_total_price("Assets:Cash -10 HOOL {183.07 USD} @@ 1979.00 USD")]
#[case::double_brace_total_and_unit_price("Assets:Cash -10 HOOL {{1830.70 USD}} @ 197.90 USD")]
#[case::double_brace_total_and_total_price("Assets:Cash -10 HOOL {{1830.70 USD}} @@ 1979.00 USD")]
#[case::hash_total_and_unit_price("Assets:Cash -10 HOOL {# 1830.70 USD} @ 197.90 USD")]
#[case::hash_total_and_total_price("Assets:Cash -10 HOOL {# 1830.70 USD} @@ 1979.00 USD")]
#[case::cost_date_and_price("Assets:Cash -10 HOOL {183.07 USD, 2024-05-12} @ 197.90 USD")]
#[case::cost_label_and_price(r#"Assets:Cash -10 HOOL {183.07 USD, "old-lot"} @ 197.90 USD"#)]
#[case::full_cost_and_price(r#"Assets:Cash -10 HOOL {183.07 USD, 2024-05-12, "lot"} @ 197.90 USD"#)]
fn should_parse_cost_and_price_together(#[case] input: &str) {
    let input = format!("2026-01-15 * \"Sell shares\"\n  {input}\n  Assets:Cash\n  Income:Gains");
    let beancount = parse::<f64>(&input).expect("should parse cost and price together");
    let DirectiveContent::Transaction(trx) = &beancount.directives[0].content else {
        panic!("not a transaction");
    };
    let posting = &trx.postings[0];
    assert!(posting.cost.is_some(), "cost should be present");
    assert!(posting.price.is_some(), "price should be present");
}

// ============================================================================
// REAL-WORLD EXAMPLES FROM SCALABLE CAPITAL
// ============================================================================

#[rstest]
#[case::scalable_etf_double_brace(
    "Assets:ScalableCapital:MsciWorld 36.78 CHF {{350.00 EUR}}",
    "ETF purchase with double-brace total cost"
)]
#[case::scalable_etf_hash(
    "Assets:ScalableCapital:MsciWorld 36.78 CHF {# 350.00 EUR}",
    "ETF purchase with hash total cost"
)]
#[case::scalable_multiple_lots(
    "Assets:ScalableCapital:MsciWorld 10.5 CHF {95.20 EUR, 2025-12-15}",
    "ETF lot with per-unit cost and date"
)]
#[case::scalable_sell_with_gains(
    r#"Assets:ScalableCapital:MsciWorld -20 CHF {95.20 EUR, "dec-2025"} @ 102.50 EUR"#,
    "Selling ETF lot with label and current price"
)]
fn should_parse_real_world_examples(#[case] input: &str, #[case] description: &str) {
    let input = format!("2026-01-15 * \"{description}\"\n  {input}\n  Assets:Cash");
    parse::<f64>(&input).unwrap_or_else(|e| panic!("Failed to parse {description}: {e}"));
}

// ============================================================================
// EDGE CASES AND SPECIAL SCENARIOS
// ============================================================================

#[rstest]
#[case::isin_as_commodity("Assets:Cash 100 DE0005140008 {50.00 EUR}", "ISIN as commodity")]
#[case::crypto_with_decimals(
    "Assets:Crypto 0.00123456 BTC {45000.00 USD}",
    "Cryptocurrency with many decimals"
)]
#[case::fractional_shares("Assets:Cash 0.5 CHF {350.00 USD}", "Fractional shares")]
#[case::large_amount("Assets:Cash 1000000 STOCK {0.001 USD}", "Large quantity")]
#[case::negative_amount("Assets:Cash -500 CHF {350.00 EUR}", "Negative amount (selling)")]
fn should_handle_edge_cases(#[case] input: &str, #[case] description: &str) {
    let input = format!("2026-01-15 * \"{description}\"\n  {input}\n  Assets:Cash");
    parse::<f64>(&input).unwrap_or_else(|e| panic!("Failed to parse {description}: {e}"));
}

#[rstest]
#[case::multiple_currencies(
    "2026-01-15 * \"Multi-currency\"\n  Assets:Cash 10 EUR {1.1 USD}\n  Assets:USD",
    "Buying EUR with USD cost basis"
)]
#[case::conversion_with_price(
    "2026-01-15 * \"Currency conversion\"\n  Assets:USD 1100 USD @ 0.909 EUR\n  Assets:EUR",
    "Currency conversion with exchange rate"
)]
fn should_handle_multi_currency_scenarios(#[case] input: &str, #[case] description: &str) {
    parse::<f64>(input).unwrap_or_else(|e| panic!("Failed to parse {description}: {e}"));
}

// ============================================================================
// INVALID SYNTAX TESTS
// These SHOULD fail according to Beancount spec
// ============================================================================

#[rstest]
#[case::cost_without_amount("Assets:Cash {350.00 EUR}", "Cost without units")]
#[case::price_without_amount("Assets:Cash @ 350.00 EUR", "Price without units")]
#[case::double_cost("Assets:Cash 10 CHF {100 EUR} {200 EUR}", "Two cost specs")]
#[case::double_price("Assets:Cash 10 CHF @ 100 EUR @ 200 EUR", "Two prices")]
#[case::price_before_cost("Assets:Cash 10 CHF @ 100 EUR {50 EUR}", "Price before cost")]
#[case::missing_space_before_at("Assets:Cash 10 CHF@ 100 EUR", "No space before @")]
#[case::missing_space_after_at("Assets:Cash 10 CHF @100 EUR", "No space after @")]
#[case::missing_space_before_cost("Assets:Cash 10 CHF{100 EUR}", "No space before {")]
#[case::unclosed_cost("Assets:Cash 10 CHF {100 EUR", "Unclosed cost brace")]
#[case::unopened_cost("Assets:Cash 10 CHF 100 EUR}", "Unopened cost brace")]
#[case::unclosed_double_brace("Assets:Cash 10 CHF {{100 EUR", "Unclosed double brace")]
#[case::mismatched_braces("Assets:Cash 10 CHF {{100 EUR}", "Mismatched braces")]
#[case::triple_brace("Assets:Cash 10 CHF {{{100 EUR}}}", "Triple brace")]
#[case::empty_price("Assets:Cash 10 CHF @", "Empty price")]
fn should_reject_invalid_cost_price_syntax(#[case] input: &str, #[case] description: &str) {
    let input = format!("2026-01-15 *\n  {input}\n  Assets:Cash");
    let result = parse::<f64>(&input);
    assert!(result.is_err(), "Should reject: {description}");
}

#[rstest]
fn should_include_tag_stack() {
    let input = r"
pushtag #foo
pushtag #bar
2022-05-27 * #baz
poptag #foo
2022-05-28 *";
    let beancount = parse::<f64>(input).unwrap();
    let transactions: Vec<_> = beancount
        .directives
        .into_iter()
        .map(|d| match d.content {
            DirectiveContent::Transaction(trx) => trx,
            _ => panic!("was not a transaction: {d:?}"),
        })
        .collect();
    assert_eq!(
        transactions.len(),
        2,
        "unexpected number of transactions: {transactions:?}"
    );
    assert_eq!(
        transactions[0]
            .tags
            .iter()
            .map(AsRef::as_ref)
            .collect::<HashSet<_>>(),
        ["foo", "bar", "baz"].into_iter().collect::<HashSet<_>>()
    );
    assert_eq!(
        transactions[1]
            .tags
            .iter()
            .map(AsRef::as_ref)
            .collect::<HashSet<_>>(),
        ["bar"].into_iter().collect::<HashSet<_>>()
    );
}

#[rstest]
fn should_reject_invalid_input(
    #[values(
        "2023-05-15txn \"narration\"",
        "2023-05-15* \"narration\"",
        "2023-05-15! \"narration\"",
        "2023-05-15 txn\"narration\"",
        "2023-05-15txn \"payee\" \"narration\"",
        "2023-05-15 txn\"payee\" \"narration\"",
        "2023-05-15 txn \"payee\"\"narration\"",
        "2023-05-15 * \"payee\"\"narration\"",
        "2023-05-15 txn\nAssets:Cash",
        "2023-05-15 * \"hello\"\nAssets:Cash",
        "2023-05-15 * \"test\"\n  *Assets:Cash",
        "2023-05-15 * \"test\"\n* Assets:Cash",
        "2023-05-15 * \"test\"\n  Assets:Cash10 CHF",
        "2023-05-15 * \"test\"\n  Assets:Cash 10CHF",
        "2023-05-15 * \"test\"\n  Assets:Cash 10..2 CHF",
        "2023-05-15 * \"test\"\n  Assets:Cash - CHF",
        "2023-05-19 *\n  Assets:Cash 1 CHF @2 EUR",
        "2023-05-19 *\n  Assets:Cash 1 CHF@ 2 EUR",
        "2023-05-19 *\n  Assets:Cash @ 2 EUR",
        "2023-05-19 *\n  Assets:Cash {1 EUR} @ 4 PLN",
        "2023-05-19 *\n  Assets:Cash {1 EUR}",
        "2023-05-19 *\n  Assets:Cash 1 CHF {1 EUR}@ 4 PLN",
        "2023-05-19 *\n  Assets:Cash 1 CHF {1 EUR} @4 PLN",
        "2023-05-19 *\n  Assets:Cash 1 CHF {1 EUR,}",
        "2023-05-19 *\n  Assets:Cash 1 CHF {, 2023-05-19}",
        "2023-05-19 *\n  Assets:Cash 1 CHF {,}",
        "pushtag#test",
        "pushtag test",
        "pushtag",
        "poptagtest",
        "poptag#test",
        "poptag test",
        "poptag",
        "poptagtest"
    )]
    input: &str,
) {
    println!("{input}");
    let result = parse::<f64>(input);
    assert!(result.is_err(), "{result:#?}");
}

#[track_caller]
fn parse_single_directive(input: &str) -> Directive<f64> {
    let directives = parse(input).expect("parsing should succeed").directives;
    assert_eq!(
        directives.len(),
        1,
        "unexpected number of directives: {directives:?}"
    );
    directives.into_iter().next().unwrap()
}

#[track_caller]
fn parse_single_posting(input: &str) -> Posting<f64> {
    let trx = parse_single_transaction(input);
    assert_eq!(
        trx.postings.len(),
        1,
        "unexpected number of postings: {:?}",
        trx.postings
    );
    trx.postings.into_iter().next().unwrap()
}

#[track_caller]
fn parse_single_transaction(input: &str) -> Transaction<f64> {
    let directive_content = parse_single_directive(input).content;
    let DirectiveContent::Transaction(trx) = directive_content else {
        panic!("was not a transaction but: {directive_content:?}");
    };
    trx
}
