use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while},
    character::complete::satisfy,
    character::complete::{char as char_tag, space0, space1},
    combinator::{cut, iterator, map, opt, value},
    sequence::{delimited, preceded, terminated},
    Parser,
};

use crate::string;
use crate::{
    account, account::Account, amount, amount::Amount, date, empty_line, end_of_line, metadata,
    Date, Decimal, IResult, Span,
};

/// A transaction
///
/// It notably contains a list of [`Posting`]
///
/// # Example
/// ```
/// # use beancount_parser::{BeancountFile, DirectiveContent};
/// let input = r#"
/// 2022-05-22 * "Grocery store" "Grocery shopping" #food
///   Assets:Cash           -10 CHF
///   Expenses:Groceries
/// "#;
///
/// let beancount: BeancountFile<f64> = input.parse().unwrap();
/// let DirectiveContent::Transaction(trx) = &beancount.directives[0].content else {
///   unreachable!("was not a transaction")
/// };
/// assert_eq!(trx.flag, Some('*'));
/// assert_eq!(trx.payee.as_deref(), Some("Grocery store"));
/// assert_eq!(trx.narration.as_deref(), Some("Grocery shopping"));
/// assert!(trx.tags.contains("food"));
/// assert_eq!(trx.postings.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Transaction<D> {
    /// Transaction flag (`*` or `!` or `None` when using the `txn` keyword)
    pub flag: Option<char>,
    /// Payee (if present)
    pub payee: Option<String>,
    /// Narration (if present)
    pub narration: Option<String>,
    /// Set of tags
    pub tags: HashSet<Tag>,
    /// Set of links
    pub links: HashSet<Link>,
    /// Postings
    pub postings: Vec<Posting<D>>,
}

impl<D: Display> Display for Transaction<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.flag {
            Some(flag) => write!(f, "{flag}")?,
            None => write!(f, "txn")?,
        }

        if let Some(payee) = &self.payee {
            write!(f, r#" "{payee}""#)?;
        }

        write!(f, r#" "{}""#, self.narration.as_deref().unwrap_or_default())?;

        // Sort tags and links for deterministic output
        let mut tags: Vec<_> = self.tags.iter().collect();
        tags.sort();
        for tag in tags {
            write!(f, " #{tag}")?;
        }

        let mut links: Vec<_> = self.links.iter().collect();
        links.sort();
        for link in links {
            write!(f, " ^{link}")?;
        }

        for posting in &self.postings {
            writeln!(f)?;
            fmt_posting(posting, "  ", f)?;
        }

        Ok(())
    }
}

/// A transaction posting
///
/// # Example
/// ```
/// # use beancount_parser::{BeancountFile, DirectiveContent, PostingPrice};
/// let input = r#"
/// 2022-05-22 * "Grocery shopping"
///   Assets:Cash           1 CHF {2 PLN} @ 3 EUR
///   Expenses:Groceries
/// "#;
///
/// let beancount: BeancountFile<f64> = input.parse().unwrap();
/// let DirectiveContent::Transaction(trx) = &beancount.directives[0].content else {
///   unreachable!("was not a transaction")
/// };
/// let posting = &trx.postings[0];
/// assert_eq!(posting.account.as_str(), "Assets:Cash");
/// assert_eq!(posting.amount.as_ref().unwrap().value, 1.0);
/// assert_eq!(posting.amount.as_ref().unwrap().currency.as_str(), "CHF");
/// assert_eq!(posting.cost.as_ref().unwrap().amount.as_ref().unwrap().value, 2.0);
/// assert_eq!(posting.cost.as_ref().unwrap().amount.as_ref().unwrap().currency.as_str(), "PLN");
/// let Some(PostingPrice::Unit(price)) = &posting.price else {
///   unreachable!("no price");
/// };
/// assert_eq!(price.value, 3.0);
/// assert_eq!(price.currency.as_str(), "EUR");
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Posting<D> {
    /// Transaction flag (`*` or `!` or `None` when absent)
    pub flag: Option<char>,
    /// Account modified by the posting
    pub account: Account,
    /// Amount being added to the account
    pub amount: Option<Amount<D>>,
    /// Cost (content within `{` and `}`)
    pub cost: Option<Cost<D>>,
    /// Price (`@` or `@@`) syntax
    pub price: Option<PostingPrice<D>>,
    /// The metadata attached to the posting
    pub metadata: metadata::Map<D>,
}

impl<D> Posting<D> {
    /// Create a new empty posting for the given account
    #[must_use]
    pub fn from_account(account: Account) -> Posting<D> {
        Posting {
            flag: None,
            account,
            amount: None,
            cost: None,
            price: None,
            metadata: metadata::Map::new(),
        }
    }
}

impl<D: Display> Display for Posting<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fmt_posting(self, "", f)
    }
}

fn fmt_posting<D: Display>(
    posting: &Posting<D>,
    indent: &str,
    f: &mut Formatter<'_>,
) -> std::fmt::Result {
    write!(f, "{indent}")?;

    if let Some(flag) = posting.flag {
        write!(f, "{flag} ")?;
    }

    write!(f, "{}", posting.account)?;

    if let Some(amount) = &posting.amount {
        write!(f, " {amount}")?;
    }

    if let Some(cost) = &posting.cost {
        write!(f, " {{")?;
        if let Some(date) = &cost.date {
            write!(f, "{date}")?;
            if cost.amount.is_some() {
                write!(f, ", ")?;
            }
        }
        if let Some(amount) = &cost.amount {
            write!(f, "{amount}")?;
        }
        write!(f, "}}")?;
    }

    if let Some(price) = &posting.price {
        match price {
            PostingPrice::Unit(amount) => write!(f, " @ {amount}")?,
            PostingPrice::Total(amount) => write!(f, " @@ {amount}")?,
        }
    }

    for (key, value) in &posting.metadata {
        write!(f, "\n{indent}  {key}: {value}")?;
    }

    Ok(())
}

/// Cost of a posting
///
/// It is the amount within `{` and `}`.
///
/// # Beancount CostSpec
/// Beancount supports specifying both per-unit and total cost:
/// - `{350.00 EUR}` - per-unit cost only (`amount`)
/// - `{# 3500.00 EUR}` or `{{3500.00 EUR}}` - total cost only (`total_amount`)
/// - `{502.12 # 9.95 USD}` - both per-unit and total cost (`amount` and `total_amount`)
/// - `{350.00 EUR, 2026-01-15}` - with acquisition date
/// - `{350.00 EUR, "lot-label"}` - with lot label
/// - `{350.00 EUR, *}` - with merge flag for average cost booking
///
/// Note: While the type system allows `amount` and `total_amount` to have different
/// currencies, Beancount does not support this and the parser should reject it.
#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct Cost<D> {
    /// Per-unit cost basis of the posting (backwards compatible)
    pub amount: Option<Amount<D>>,
    /// Total cost basis (for `#` or `{{}}` syntax)
    pub total_amount: Option<Amount<D>>,
    /// The date of this cost basis
    pub date: Option<Date>,
    /// Lot label for identifying specific lots
    pub label: Option<String>,
    /// Merge flag for average cost booking
    pub merge: bool,
}

/// Price of a posting
///
/// It is the amount following the `@` or `@@` symbols
#[derive(Debug, Clone, PartialEq)]
pub enum PostingPrice<D> {
    /// Unit cost (`@`)
    Unit(Amount<D>),
    /// Total cost (`@@`)
    Total(Amount<D>),
}

/// Transaction tag
///
/// # Example
/// ```
/// # use beancount_parser::{BeancountFile, DirectiveContent};
/// let input = r#"
/// 2022-05-22 * "Grocery store" "Grocery shopping" #food
///   Assets:Cash           -10 CHF
///   Expenses:Groceries
/// "#;
///
/// let beancount: BeancountFile<f64> = input.parse().unwrap();
/// let DirectiveContent::Transaction(trx) = &beancount.directives[0].content else {
///   unreachable!("was not a transaction")
/// };
/// assert!(trx.tags.contains("food"));
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Tag(Arc<str>);

impl Tag {
    /// Returns underlying string representation
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Borrow<str> for Tag {
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

/// Transaction link
///
/// # Example
/// ```
/// # use beancount_parser::{BeancountFile, DirectiveContent};
/// let input = r#"
/// 2014-02-05 * "Invoice for January" ^invoice-pepe-studios-jan14
///    Income:Clients:PepeStudios           -8450.00 USD
///    Assets:AccountsReceivable
/// "#;
///
/// let beancount: BeancountFile<f64> = input.parse().unwrap();
/// let DirectiveContent::Transaction(trx) = &beancount.directives[0].content else {
///   unreachable!("was not a transaction")
/// };
/// assert!(trx.links.contains("invoice-pepe-studios-jan14"));
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Link(Arc<str>);

impl Link {
    /// Returns underlying string representation
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl AsRef<str> for Link {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Borrow<str> for Link {
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn parse<D: Decimal>(
    input: Span<'_>,
) -> IResult<'_, (Transaction<D>, metadata::Map<D>)> {
    let (input, flag) = alt((map(flag, Some), value(None, tag("txn")))).parse(input)?;
    cut(do_parse(flag)).parse(input)
}

fn flag(input: Span<'_>) -> IResult<'_, char> {
    satisfy(|c: char| !c.is_ascii_lowercase())(input)
}

fn do_parse<D: Decimal>(
    flag: Option<char>,
) -> impl Fn(Span<'_>) -> IResult<'_, (Transaction<D>, metadata::Map<D>)> {
    move |input| {
        let (input, payee_and_narration) =
            opt(preceded(space1, payee_and_narration)).parse(input)?;
        let (input, (tags, links)) = tags_and_links(input)?;
        let (input, ()) = end_of_line(input)?;
        let (input, metadata) = metadata::parse(input)?;
        let mut iter = iterator(input, alt((posting.map(Some), empty_line.map(|()| None))));
        let postings = iter.by_ref().flatten().collect();
        let (input, ()) = iter.finish()?;
        let (payee, narration) = match payee_and_narration {
            Some((payee, narration)) => (payee, Some(narration)),
            None => (None, None),
        };
        Ok((
            input,
            (
                Transaction {
                    flag,
                    payee,
                    narration,
                    tags,
                    links,
                    postings,
                },
                metadata,
            ),
        ))
    }
}

pub(super) enum TagOrLink {
    Tag(Tag),
    Link(Link),
}

pub(super) fn parse_tag(input: Span<'_>) -> IResult<'_, Tag> {
    map(
        preceded(
            char_tag('#'),
            take_while(|c: char| c.is_alphanumeric() || c == '-' || c == '_'),
        ),
        |s: Span<'_>| Tag((*s.fragment()).into()),
    )
    .parse(input)
}

pub(super) fn parse_link(input: Span<'_>) -> IResult<'_, Link> {
    map(
        preceded(
            char_tag('^'),
            take_while(|c: char| c.is_alphanumeric() || c == '-' || c == '_' || c == '.'),
        ),
        |s: Span<'_>| Link((*s.fragment()).into()),
    )
    .parse(input)
}

pub(super) fn parse_tag_or_link(input: Span<'_>) -> IResult<'_, TagOrLink> {
    alt((
        map(parse_tag, TagOrLink::Tag),
        map(parse_link, TagOrLink::Link),
    ))
    .parse(input)
}

fn tags_and_links(input: Span<'_>) -> IResult<'_, (HashSet<Tag>, HashSet<Link>)> {
    let mut tags_and_links_iter = iterator(input, preceded(space0, parse_tag_or_link));
    let (tags, links) = tags_and_links_iter.by_ref().fold(
        (HashSet::new(), HashSet::new()),
        |(mut tags, mut links), x| {
            match x {
                TagOrLink::Tag(tag) => tags.insert(tag),
                TagOrLink::Link(link) => links.insert(link),
            };
            (tags, links)
        },
    );
    let (input, ()) = tags_and_links_iter.finish()?;
    Ok((input, (tags, links)))
}

fn payee_and_narration(input: Span<'_>) -> IResult<'_, (Option<String>, String)> {
    let (input, s1) = string(input)?;
    let (input, s2) = opt(preceded(space1, string)).parse(input)?;
    Ok((
        input,
        match s2 {
            Some(narration) => (Some(s1), narration),
            None => (None, s1),
        },
    ))
}

fn posting<D: Decimal>(input: Span<'_>) -> IResult<'_, Posting<D>> {
    let (input, _) = space1(input)?;
    let (input, flag) = opt(terminated(flag, space1)).parse(input)?;
    let (input, account) = account::parse(input)?;
    let (input, amounts) = opt((
        preceded(space1, amount::parse),
        opt(preceded(space1, cost)),
        opt(preceded(
            space1,
            alt((
                map(
                    preceded((char_tag('@'), space1), amount::parse),
                    PostingPrice::Unit,
                ),
                map(
                    preceded((tag("@@"), space1), amount::parse),
                    PostingPrice::Total,
                ),
            )),
        )),
    ))
    .parse(input)?;
    let (input, ()) = end_of_line(input)?;
    let (input, metadata) = metadata::parse(input)?;
    let (amount, cost, price) = match amounts {
        Some((a, l, p)) => (Some(a), l, p),
        None => (None, None, None),
    };
    Ok((
        input,
        Posting {
            flag,
            account,
            amount,
            cost,
            price,
            metadata,
        },
    ))
}

/// Parse a cost specification within
/// - {350.00 EUR} - per-unit cost
/// - {# 3500.00 EUR} or {{3500.00 EUR}} - total cost
/// - {350.00 # 3500.00 EUR} - both per-unit and total
/// - {350.00 EUR, 2026-01-15} - with date
/// - {350.00 EUR, "label"} - with label
/// - {350.00 EUR, *} - with merge flag
fn cost<D: Decimal>(input: Span<'_>) -> IResult<'_, Cost<D>> {
    let double_brace = tag("{{").parse(input);

    if double_brace.is_ok() {
        let (input, _) = double_brace?;
        let (input, _) = space0(input)?;
        let (input, total) = amount::parse(input)?;
        let (input, _) = space0(input)?;

        let mut date = None;
        let mut label = None;
        let mut merge = false;
        let mut input = input;

        loop {
            let (new_input, comma) = opt(delimited(space0, char_tag(','), space0)).parse(input)?;

            if comma.is_none() {
                break;
            }

            // A comma was found - the next component MUST exist
            let (new_input, component) = parse_cost_component::<D>(new_input)?;

            match component {
                CostComponent::Date(d) => date = Some(d),
                CostComponent::Label(l) => label = Some(l),
                CostComponent::Merge => merge = true,
                // Double-brace shouldn't have amount components
                CostComponent::PerUnitAmount(_)
                | CostComponent::TotalAmount(_)
                | CostComponent::PerUnitAndTotal(_, _) => {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::Tag,
                    )));
                }
            }
            input = new_input;
        }

        let (input, _) = space0(input)?;
        let (input, _) = tag("}}")(input)?;

        return Ok((
            input,
            Cost {
                amount: None,
                total_amount: Some(total),
                date,
                label,
                merge,
            },
        ));
    }

    let (input, _) = char_tag('{')(input)?;
    let (input, _) = space0(input)?;

    let (input, components) = parse_cost_components(input)?;

    let (input, _) = space0(input)?;
    let (input, _) = char_tag('}')(input)?;

    Ok((input, components))
}

/// Parse all cost components
fn parse_cost_components<D: Decimal>(input: Span<'_>) -> IResult<'_, Cost<D>> {
    let mut amount = None;
    let mut total_amount = None;
    let mut date = None;
    let mut label = None;
    let mut merge = false;

    let (mut input, first) = opt(parse_cost_component).parse(input)?;

    if let Some(component) = first {
        match component {
            CostComponent::PerUnitAmount(a) => amount = Some(a),
            CostComponent::TotalAmount(a) => total_amount = Some(a),
            CostComponent::PerUnitAndTotal(a, t) => {
                amount = Some(a);
                total_amount = Some(t);
            }
            CostComponent::Date(d) => date = Some(d),
            CostComponent::Label(l) => label = Some(l),
            CostComponent::Merge => merge = true,
        }
    } else {
        // If first component is None, ensure there's no leading comma
        let (_, peek_comma) = opt(delimited(space0, char_tag(','), space0)).parse(input)?;
        if peek_comma.is_some() {
            // Leading comma detected - fail
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    }

    // Parse remaining components separated by commas
    loop {
        let (new_input, comma) = opt(delimited(space0, char_tag(','), space0)).parse(input)?;

        if comma.is_none() {
            break;
        }

        // A comma was found - the next component MUST exist (no trailing/leading commas)
        let (new_input, component) = parse_cost_component(new_input)?;

        match component {
            CostComponent::PerUnitAmount(a) => amount = Some(a),
            CostComponent::TotalAmount(a) => total_amount = Some(a),
            CostComponent::PerUnitAndTotal(a, t) => {
                amount = Some(a);
                total_amount = Some(t);
            }
            CostComponent::Date(d) => date = Some(d),
            CostComponent::Label(l) => label = Some(l),
            CostComponent::Merge => merge = true,
        }
        input = new_input;
    }

    Ok((
        input,
        Cost {
            amount,
            total_amount,
            date,
            label,
            merge,
        },
    ))
}

enum CostComponent<D> {
    PerUnitAmount(Amount<D>),
    TotalAmount(Amount<D>),
    PerUnitAndTotal(Amount<D>, Amount<D>),
    Date(Date),
    Label(String),
    Merge,
}
fn parse_cost_component<D: Decimal>(input: Span<'_>) -> IResult<'_, CostComponent<D>> {
    alt((
        map(char_tag('*'), |_| CostComponent::Merge),
        map(string, CostComponent::Label),
        map(date::parse, CostComponent::Date),
        parse_amount_component,
    ))
    .parse(input)
}

/// - `350.00 EUR` - per-unit cost only
/// - `# 3500.00 EUR` - total cost only
/// - `350.00 # 3500.00 EUR` - both per-unit and total (returns special marker)
fn parse_amount_component<D: Decimal>(input: Span<'_>) -> IResult<'_, CostComponent<D>> {
    // Check for # prefix (total cost only)
    // Note: Beancount requires space after # to avoid ambiguity with tags
    let hash = preceded(space0, char_tag('#')).parse(input);

    if hash.is_ok() {
        let (input, _) = hash?;
        let (input, _) = space1(input)?; // Require at least one space after #
        let (input, total) = amount::parse(input)?;
        return Ok((input, CostComponent::TotalAmount(total)));
    }

    // Try to parse: number # number currency (both per-unit and total)
    let both_result = parse_per_unit_and_total(input);
    if let Ok((input, (per_unit, total))) = both_result {
        // We have both! But CostComponent can only return one...
        // We need a way to return both.
        return Ok((input, CostComponent::PerUnitAndTotal(per_unit, total)));
    }

    // Otherwise, regular per-unit amount with currency
    let (input, per_unit) = amount::parse(input)?;
    Ok((input, CostComponent::PerUnitAmount(per_unit)))
}

/// Parse the special case: number # number currency
/// Example: 502.12 # 9.95 USD
fn parse_per_unit_and_total<D: Decimal>(input: Span<'_>) -> IResult<'_, (Amount<D>, Amount<D>)> {
    let (input, per_unit_value) = amount::expression(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char_tag('#')(input)?;
    let (input, _) = space0(input)?;
    let (input, total_value) = amount::expression(input)?;
    let (input, _) = space1(input)?;
    let (input, currency) = amount::currency(input)?;

    Ok((
        input,
        (
            Amount {
                value: per_unit_value,
                currency: currency.clone(),
            },
            Amount {
                value: total_value,
                currency,
            },
        ),
    ))
}
