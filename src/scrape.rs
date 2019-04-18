use select::document::{Document};
use select::predicate::{Attr, Name};
use select::node::Node;

use chrono::DateTime;
use chrono::prelude::*;
use std::num::ParseIntError;

use reqwest::Url;
use reqwest::UrlError;

/// TribalWars Event
#[derive(Debug)]
pub struct TwEvent {
    pub place: String,
    pub points: i32,
    pub old_holder: String,
    pub new_holder: String,
    pub time: Option<DateTime<Utc>>,
}


custom_error!{pub TwEventParseError
              DateParse{source: chrono::ParseError} = "Failed to parse date: {source}",
              PointsParse{source: ParseIntError} = "Failed to parse point data: {source}",
              ValueMissing{val: String, row: String} = "Missing value for: {val} in {row}",
              NoEvents = "No events found!",
              RequestError{source: reqwest::Error} = "Error while making request: {source}",
              UrlError{source: UrlError} = "Error parsing url for request: {source}"
}

fn parse_row(row: Node<'_>) -> Result<TwEvent, TwEventParseError> {

    let mut itr = row.find(Name("td")).take(5).map(|t| t.text());

    use self::TwEventParseError::ValueMissing;

    macro_rules! try_next {
        ($id:ident) => {
            let $id = itr.next().ok_or(ValueMissing { val: stringify!(id).to_string(), row: row.html() })?;
        }
    }

    try_next!(place);
    let point_str = { try_next!(point_str); str::replace(&point_str, ",", "") };
    try_next!(old_holder);
    try_next!(new_holder);
    try_next!(time_str);

    let time = Utc.datetime_from_str(time_str.as_str(), "%Y-%m-%d - %H:%M:%S").ok();
    let points : i32 = point_str.parse()?;

    Ok(TwEvent { place, points, old_holder, new_holder, time })
}

fn parse_doc(docstr: &str) -> Result<Vec<TwEvent>, TwEventParseError> {
    let document = Document::from(docstr);
    
    if let Some(table) = document.find(Attr("class","widget")).next() {
        return table.find(Name("tr"))
            .skip(1) // skip header row
            .map(parse_row)
            .collect::<Result<Vec<TwEvent>, TwEventParseError>>();
    }

    Err(TwEventParseError::NoEvents)
}

pub fn get_and_parse_site(url: Url) -> Result<Vec<TwEvent>, TwEventParseError> {
    let resp = reqwest::get(url)?.text()?;
    
    parse_doc(&resp)
}
