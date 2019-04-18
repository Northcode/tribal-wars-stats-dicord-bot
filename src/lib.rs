extern crate reqwest;
extern crate select;
extern crate chrono;

#[macro_use]
extern crate custom_error;

mod scrape;


#[cfg(test)]
mod tests
{
    use super::*;
    use reqwest::Url;

    #[test]
    fn test_get_and_parse_site() {
        let url : Url = "http://de.twstats.com/de152/index.php?page=ennoblements&live=live".parse().unwrap();

        let result = scrape::get_and_parse_site(url);

        assert!(result.is_ok());
    }
}
