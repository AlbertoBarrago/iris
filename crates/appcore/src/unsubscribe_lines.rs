//! The words of the one dialog that asks before Iris leaves a mailing
//! list, about plain data: each list, the way out of it, and what leaving
//! will do. The window draws the dialog; these sentences are shared by it
//! and the assistant, and are tested as such.

use mailrs_domain::translate::{fill, fill_plural, gettext};

use crate::unsubscribe_page::{Outcome, Prepared, Step};

/// One list on the dialog, and what leaving it will do.
pub struct ListLine {
    /// The sender as a person reads it, "Trail Notes".
    pub name: String,
    pub way: Way,
}

/// The way out of one list, as far as it is known.
pub enum Way {
    /// The page is still being read. The line carries a spinner until
    /// one of the others arrives to take its place.
    Reading,
    /// The sender promised RFC 8058, and one request is the whole of it.
    OneClick,
    /// A request goes out as mail from `from`.
    Mail { from: String },
    /// The page was read and decided on.
    Page(Prepared),
}

pub fn anyone_reading(ways: &[Way]) -> bool {
    ways.iter().any(|way| matches!(way, Way::Reading))
}

/// The question at the top. One list is named; several are counted.
pub fn heading(lines: &[ListLine]) -> String {
    match lines {
        [only] => fill(
            &gettext("Unsubscribe from {sender}?"),
            &[("sender", &only.name)],
        ),
        many => fill_plural(
            "Unsubscribe from {count} list?",
            "Unsubscribe from {count} lists?",
            many.len(),
            &[("count", &many.len().to_string())],
        ),
    }
}

/// The line under the question, for a dialog that will load a page.
/// Loading one is an act the sender can see, which is worth saying
/// before it happens rather than after.
pub fn body(lines: &[ListLine]) -> Option<String> {
    let page = lines
        .iter()
        .any(|line| matches!(line.way, Way::Reading | Way::Page(_)));
    page.then(|| gettext("Loading a page tells the sender you acted, as opening it yourself does."))
}

/// What one line says will happen, under the list's name.
pub fn line_text(way: &Way) -> String {
    match way {
        Way::Reading => gettext("Reading the page…"),
        Way::OneClick => gettext("ask the sender to take you off the list"),
        Way::Mail { from } => fill(
            &gettext("send a request from {address}"),
            &[("address", &mask(from))],
        ),
        Way::Page(prepared) => page_text(prepared),
    }
}

/// What a page that has been read will have done to it. A page the rules
/// and the model both gave up on is the one the person finishes
/// themselves, in their own browser, as they did before any of this.
fn page_text(prepared: &Prepared) -> String {
    let plan = match &prepared.step {
        Step::AlreadyOff => return gettext("nothing: the page says you are off the list already"),
        Step::Browser(_) => return gettext("open the page in the browser"),
        Step::Submit(plan) => plan,
    };
    let button = &prepared.button;
    let site = site(&prepared.url);
    let address = mask(&prepared.address);
    let values = [
        ("button", button.as_str()),
        ("site", site.as_str()),
        ("address", address.as_str()),
    ];
    match (!plan.fill.is_empty(), !plan.tick.is_empty()) {
        (true, true) => fill(
            &gettext("press “{button}” on {site} with {address}, all emails"),
            &values,
        ),
        (true, false) => fill(
            &gettext("press “{button}” on {site} with {address}"),
            &values,
        ),
        (false, true) => fill(&gettext("press “{button}” on {site}, all emails"), &values),
        (false, false) => fill(&gettext("press “{button}” on {site}"), &values),
    }
}

/// The toast after the window's Unsubscribe has run, naming the list the
/// person just read on the dialog. The assistant answers in its own
/// words, so only one list ever reaches a toast.
pub fn summary(name: &str, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Done => fill(&gettext("Unsubscribed from {sender}"), &[("sender", name)]),
        Outcome::Unclear(_) => gettext("Sent, but the page did not say it worked"),
        Outcome::OpenInBrowser(_) => gettext("The page needs you to finish it"),
        Outcome::Failed(why) => fill(
            &gettext("Could not unsubscribe from {sender}: {reason}"),
            &[("sender", name), ("reason", why)],
        ),
    }
}

/// An address with all but its first letter hidden, "d…@gmail.com". The
/// dialog says which address goes into a page, and the whole of it
/// belongs on screen no more than a password does.
pub fn mask(address: &str) -> String {
    let Some((who, domain)) = address.split_once('@') else {
        return address.to_string();
    };
    let mut letters = who.chars();
    match letters.next() {
        Some(first) if letters.next().is_some() => format!("{first}…@{domain}"),
        _ => address.to_string(),
    }
}

/// The site a page belongs to, "news.shop.com". The scheme, the path and
/// the token an unsubscribe link carries say nothing a person reading a
/// dialog wants.
fn site(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    let host = match host.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => host,
    };
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

/// The address a newsletter was sent to: the first of `to_and_cc` the
/// account may send mail as, and the account's own address when none of
/// them is. It is the only text Iris types into a page, so it
/// has to be one of the person's own addresses rather than whatever the
/// message happens to name.
pub fn sent_to(to_and_cc: &[String], send_as: &[String], account: &str) -> String {
    to_and_cc
        .iter()
        .find(|address| {
            send_as
                .iter()
                .any(|mine| mine.eq_ignore_ascii_case(address))
        })
        .cloned()
        .unwrap_or_else(|| account.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unsubscribe_page::Plan;

    fn prepared(step: Step, button: &str) -> Prepared {
        Prepared {
            url: "https://news.shop.com/u/9f2?t=abc".to_string(),
            address: "dana@gmail.com".to_string(),
            button: button.to_string(),
            step,
        }
    }

    fn plan(fill: bool, tick: bool) -> Step {
        Step::Submit(Plan {
            form: 0,
            fill: match fill {
                true => vec![(1, "dana@gmail.com".to_string())],
                false => Vec::new(),
            },
            tick: match tick {
                true => vec![2],
                false => Vec::new(),
            },
            press: 3,
        })
    }

    #[test]
    fn a_page_line_names_the_button_the_site_and_the_address() {
        assert_eq!(
            line_text(&Way::Page(prepared(plan(true, true), "Unsubscribe"))),
            "press “Unsubscribe” on news.shop.com with d…@gmail.com, all emails"
        );
        assert_eq!(
            line_text(&Way::Page(prepared(plan(true, false), "Confirm"))),
            "press “Confirm” on news.shop.com with d…@gmail.com"
        );
        assert_eq!(
            line_text(&Way::Page(prepared(plan(false, false), "Unsubscribe"))),
            "press “Unsubscribe” on news.shop.com"
        );
    }

    #[test]
    fn a_page_nobody_could_read_says_it_goes_to_the_browser() {
        let url = "https://news.shop.com/u/9f2".to_string();
        assert_eq!(
            line_text(&Way::Page(prepared(Step::Browser(url), ""))),
            "open the page in the browser"
        );
        assert_eq!(
            line_text(&Way::Page(prepared(Step::AlreadyOff, ""))),
            "nothing: the page says you are off the list already"
        );
    }

    #[test]
    fn the_other_two_ways_out_say_what_they_do() {
        assert_eq!(
            line_text(&Way::OneClick),
            "ask the sender to take you off the list"
        );
        assert_eq!(
            line_text(&Way::Mail {
                from: "dana@gmail.com".to_string()
            }),
            "send a request from d…@gmail.com"
        );
        assert_eq!(line_text(&Way::Reading), "Reading the page…");
    }

    #[test]
    fn an_address_keeps_its_first_letter_and_its_domain() {
        assert_eq!(mask("dana@gmail.com"), "d…@gmail.com");
        // Nothing worth hiding, and nothing to hide it behind.
        assert_eq!(mask("d@gmail.com"), "d@gmail.com");
        assert_eq!(mask("not an address"), "not an address");
    }

    #[test]
    fn a_site_is_the_host_without_the_rest_of_the_link() {
        assert_eq!(
            site("https://www.news.shop.com/u/9f2?t=abc"),
            "news.shop.com"
        );
        assert_eq!(site("http://list.example:8080/out"), "list.example");
        assert_eq!(site("https://news.shop.com"), "news.shop.com");
    }

    #[test]
    fn a_list_left_is_named() {
        assert_eq!(
            summary("Trail Notes", &Outcome::Done),
            "Unsubscribed from Trail Notes"
        );
    }

    #[test]
    fn one_list_that_did_not_work_says_which_and_why() {
        assert_eq!(
            summary(
                "Trail Notes",
                &Outcome::Failed("the page took longer than 20 seconds".into())
            ),
            "Could not unsubscribe from Trail Notes: the page took longer than 20 seconds"
        );
        assert_eq!(
            summary("Trail Notes", &Outcome::Unclear(String::new())),
            "Sent, but the page did not say it worked"
        );
    }

    #[test]
    fn the_address_a_newsletter_came_to_is_one_of_the_persons_own() {
        let mine = [
            "dana@gmail.com".to_string(),
            "dana@studio.example".to_string(),
        ];
        let sent = [
            "newsletter@shop.example".to_string(),
            "DANA@studio.example".to_string(),
        ];
        assert_eq!(
            sent_to(&sent, &mine, "dana@gmail.com"),
            "DANA@studio.example"
        );
        // A list that carries nobody's address in the open falls back to
        // the account it arrived in.
        let hidden = ["undisclosed-recipients:;".to_string()];
        assert_eq!(sent_to(&hidden, &mine, "dana@gmail.com"), "dana@gmail.com");
    }
}
