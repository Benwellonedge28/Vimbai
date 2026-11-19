#![forbid(unsafe_code)]
//! ISO 4217: every currency in the world a Book may keep its money in, held as data rather than as code.
//!
//! A currency is not a name and a symbol alone: it declares how many decimal places its money has, and that is what
//! makes an amount exact. A yen has none, a Kuwaiti dinar has three, and a Book that added a currency without saying
//! which would round its money to the wrong place. So the catalogue is the one place that answers it, and adding a
//! currency is a row here rather than a change to any posting path.

use serde_json::{json, Value};

/// One currency, as the world states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsoCurrency {
    /// The three-letter code, which is what identifies the money.
    pub code: &'static str,
    /// The name the world gives it.
    pub name: &'static str,
    /// How many decimal places its money has: 0 for a yen, 2 for a dollar, 3 for a dinar.
    pub minor_units: i64,
}

/// Every currency a Book may hold. Sorted by code so a search is a scan of a sorted table.
pub const ISO_4217: &[IsoCurrency] = &[
    IsoCurrency {
        code: "AED",
        name: "UAE Dirham",
        minor_units: 2,
    },
    IsoCurrency {
        code: "AFN",
        name: "Afghan Afghani",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ALL",
        name: "Albanian Lek",
        minor_units: 2,
    },
    IsoCurrency {
        code: "AMD",
        name: "Armenian Dram",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ANG",
        name: "Netherlands Antillean Guilder",
        minor_units: 2,
    },
    IsoCurrency {
        code: "AOA",
        name: "Angolan Kwanza",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ARS",
        name: "Argentine Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "AUD",
        name: "Australian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "AWG",
        name: "Aruban Florin",
        minor_units: 2,
    },
    IsoCurrency {
        code: "AZN",
        name: "Azerbaijani Manat",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BAM",
        name: "Bosnia-Herzegovina Convertible Mark",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BBD",
        name: "Barbadian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BDT",
        name: "Bangladeshi Taka",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BGN",
        name: "Bulgarian Lev",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BHD",
        name: "Bahraini Dinar",
        minor_units: 3,
    },
    IsoCurrency {
        code: "BIF",
        name: "Burundian Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "BMD",
        name: "Bermudian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BND",
        name: "Brunei Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BOB",
        name: "Bolivian Boliviano",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BRL",
        name: "Brazilian Real",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BSD",
        name: "Bahamian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BTN",
        name: "Bhutanese Ngultrum",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BWP",
        name: "Botswana Pula",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BYN",
        name: "Belarusian Ruble",
        minor_units: 2,
    },
    IsoCurrency {
        code: "BZD",
        name: "Belize Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CAD",
        name: "Canadian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CDF",
        name: "Congolese Franc",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CHF",
        name: "Swiss Franc",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CLP",
        name: "Chilean Peso",
        minor_units: 0,
    },
    IsoCurrency {
        code: "CNY",
        name: "Chinese Yuan",
        minor_units: 2,
    },
    IsoCurrency {
        code: "COP",
        name: "Colombian Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CRC",
        name: "Costa Rican Colon",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CUP",
        name: "Cuban Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CVE",
        name: "Cape Verdean Escudo",
        minor_units: 2,
    },
    IsoCurrency {
        code: "CZK",
        name: "Czech Koruna",
        minor_units: 2,
    },
    IsoCurrency {
        code: "DJF",
        name: "Djiboutian Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "DKK",
        name: "Danish Krone",
        minor_units: 2,
    },
    IsoCurrency {
        code: "DOP",
        name: "Dominican Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "DZD",
        name: "Algerian Dinar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "EGP",
        name: "Egyptian Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ERN",
        name: "Eritrean Nakfa",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ETB",
        name: "Ethiopian Birr",
        minor_units: 2,
    },
    IsoCurrency {
        code: "EUR",
        name: "Euro",
        minor_units: 2,
    },
    IsoCurrency {
        code: "FJD",
        name: "Fijian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "FKP",
        name: "Falkland Islands Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GBP",
        name: "British Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GEL",
        name: "Georgian Lari",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GHS",
        name: "Ghanaian Cedi",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GIP",
        name: "Gibraltar Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GMD",
        name: "Gambian Dalasi",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GNF",
        name: "Guinean Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "GTQ",
        name: "Guatemalan Quetzal",
        minor_units: 2,
    },
    IsoCurrency {
        code: "GYD",
        name: "Guyanese Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "HKD",
        name: "Hong Kong Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "HNL",
        name: "Honduran Lempira",
        minor_units: 2,
    },
    IsoCurrency {
        code: "HTG",
        name: "Haitian Gourde",
        minor_units: 2,
    },
    IsoCurrency {
        code: "HUF",
        name: "Hungarian Forint",
        minor_units: 2,
    },
    IsoCurrency {
        code: "IDR",
        name: "Indonesian Rupiah",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ILS",
        name: "Israeli New Shekel",
        minor_units: 2,
    },
    IsoCurrency {
        code: "INR",
        name: "Indian Rupee",
        minor_units: 2,
    },
    IsoCurrency {
        code: "IQD",
        name: "Iraqi Dinar",
        minor_units: 3,
    },
    IsoCurrency {
        code: "IRR",
        name: "Iranian Rial",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ISK",
        name: "Icelandic Krona",
        minor_units: 0,
    },
    IsoCurrency {
        code: "JMD",
        name: "Jamaican Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "JOD",
        name: "Jordanian Dinar",
        minor_units: 3,
    },
    IsoCurrency {
        code: "JPY",
        name: "Japanese Yen",
        minor_units: 0,
    },
    IsoCurrency {
        code: "KES",
        name: "Kenyan Shilling",
        minor_units: 2,
    },
    IsoCurrency {
        code: "KGS",
        name: "Kyrgyzstani Som",
        minor_units: 2,
    },
    IsoCurrency {
        code: "KHR",
        name: "Cambodian Riel",
        minor_units: 2,
    },
    IsoCurrency {
        code: "KMF",
        name: "Comorian Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "KPW",
        name: "North Korean Won",
        minor_units: 2,
    },
    IsoCurrency {
        code: "KRW",
        name: "South Korean Won",
        minor_units: 0,
    },
    IsoCurrency {
        code: "KWD",
        name: "Kuwaiti Dinar",
        minor_units: 3,
    },
    IsoCurrency {
        code: "KYD",
        name: "Cayman Islands Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "KZT",
        name: "Kazakhstani Tenge",
        minor_units: 2,
    },
    IsoCurrency {
        code: "LAK",
        name: "Lao Kip",
        minor_units: 2,
    },
    IsoCurrency {
        code: "LBP",
        name: "Lebanese Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "LKR",
        name: "Sri Lankan Rupee",
        minor_units: 2,
    },
    IsoCurrency {
        code: "LRD",
        name: "Liberian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "LSL",
        name: "Lesotho Loti",
        minor_units: 2,
    },
    IsoCurrency {
        code: "LYD",
        name: "Libyan Dinar",
        minor_units: 3,
    },
    IsoCurrency {
        code: "MAD",
        name: "Moroccan Dirham",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MDL",
        name: "Moldovan Leu",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MGA",
        name: "Malagasy Ariary",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MKD",
        name: "Macedonian Denar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MMK",
        name: "Myanmar Kyat",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MNT",
        name: "Mongolian Tugrik",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MOP",
        name: "Macanese Pataca",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MRU",
        name: "Mauritanian Ouguiya",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MUR",
        name: "Mauritian Rupee",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MVR",
        name: "Maldivian Rufiyaa",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MWK",
        name: "Malawian Kwacha",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MXN",
        name: "Mexican Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MYR",
        name: "Malaysian Ringgit",
        minor_units: 2,
    },
    IsoCurrency {
        code: "MZN",
        name: "Mozambican Metical",
        minor_units: 2,
    },
    IsoCurrency {
        code: "NAD",
        name: "Namibian Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "NGN",
        name: "Nigerian Naira",
        minor_units: 2,
    },
    IsoCurrency {
        code: "NIO",
        name: "Nicaraguan Cordoba",
        minor_units: 2,
    },
    IsoCurrency {
        code: "NOK",
        name: "Norwegian Krone",
        minor_units: 2,
    },
    IsoCurrency {
        code: "NPR",
        name: "Nepalese Rupee",
        minor_units: 2,
    },
    IsoCurrency {
        code: "NZD",
        name: "New Zealand Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "OMR",
        name: "Omani Rial",
        minor_units: 3,
    },
    IsoCurrency {
        code: "PAB",
        name: "Panamanian Balboa",
        minor_units: 2,
    },
    IsoCurrency {
        code: "PEN",
        name: "Peruvian Sol",
        minor_units: 2,
    },
    IsoCurrency {
        code: "PGK",
        name: "Papua New Guinean Kina",
        minor_units: 2,
    },
    IsoCurrency {
        code: "PHP",
        name: "Philippine Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "PKR",
        name: "Pakistani Rupee",
        minor_units: 2,
    },
    IsoCurrency {
        code: "PLN",
        name: "Polish Zloty",
        minor_units: 2,
    },
    IsoCurrency {
        code: "PYG",
        name: "Paraguayan Guarani",
        minor_units: 0,
    },
    IsoCurrency {
        code: "QAR",
        name: "Qatari Riyal",
        minor_units: 2,
    },
    IsoCurrency {
        code: "RON",
        name: "Romanian Leu",
        minor_units: 2,
    },
    IsoCurrency {
        code: "RSD",
        name: "Serbian Dinar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "RUB",
        name: "Russian Ruble",
        minor_units: 2,
    },
    IsoCurrency {
        code: "RWF",
        name: "Rwandan Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "SAR",
        name: "Saudi Riyal",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SBD",
        name: "Solomon Islands Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SCR",
        name: "Seychellois Rupee",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SDG",
        name: "Sudanese Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SEK",
        name: "Swedish Krona",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SGD",
        name: "Singapore Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SHP",
        name: "Saint Helena Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SLE",
        name: "Sierra Leonean Leone",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SOS",
        name: "Somali Shilling",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SRD",
        name: "Surinamese Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SSP",
        name: "South Sudanese Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "STN",
        name: "Sao Tome Dobra",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SVC",
        name: "Salvadoran Colon",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SYP",
        name: "Syrian Pound",
        minor_units: 2,
    },
    IsoCurrency {
        code: "SZL",
        name: "Swazi Lilangeni",
        minor_units: 2,
    },
    IsoCurrency {
        code: "THB",
        name: "Thai Baht",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TJS",
        name: "Tajikistani Somoni",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TMT",
        name: "Turkmenistani Manat",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TND",
        name: "Tunisian Dinar",
        minor_units: 3,
    },
    IsoCurrency {
        code: "TOP",
        name: "Tongan Paanga",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TRY",
        name: "Turkish Lira",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TTD",
        name: "Trinidad and Tobago Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TWD",
        name: "New Taiwan Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "TZS",
        name: "Tanzanian Shilling",
        minor_units: 2,
    },
    IsoCurrency {
        code: "UAH",
        name: "Ukrainian Hryvnia",
        minor_units: 2,
    },
    IsoCurrency {
        code: "UGX",
        name: "Ugandan Shilling",
        minor_units: 0,
    },
    IsoCurrency {
        code: "USD",
        name: "US Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "UYU",
        name: "Uruguayan Peso",
        minor_units: 2,
    },
    IsoCurrency {
        code: "UZS",
        name: "Uzbekistani Som",
        minor_units: 2,
    },
    IsoCurrency {
        code: "VES",
        name: "Venezuelan Bolivar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "VND",
        name: "Vietnamese Dong",
        minor_units: 0,
    },
    IsoCurrency {
        code: "VUV",
        name: "Vanuatu Vatu",
        minor_units: 0,
    },
    IsoCurrency {
        code: "WST",
        name: "Samoan Tala",
        minor_units: 2,
    },
    IsoCurrency {
        code: "XAF",
        name: "Central African CFA Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "XCD",
        name: "East Caribbean Dollar",
        minor_units: 2,
    },
    IsoCurrency {
        code: "XOF",
        name: "West African CFA Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "XPF",
        name: "CFP Franc",
        minor_units: 0,
    },
    IsoCurrency {
        code: "YER",
        name: "Yemeni Rial",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ZAR",
        name: "South African Rand",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ZMW",
        name: "Zambian Kwacha",
        minor_units: 2,
    },
    IsoCurrency {
        // The money Zimbabwe actually uses since April 2024: the ZiG. ISO 4217 still lists only the old ZWL, so a
        // catalogue copied from the standard alone would leave the country this product was built in unable to name its
        // own money. Both are kept: ZWG for what is in use, ZWL as history.
        code: "ZWG",
        name: "Zimbabwe Gold",
        minor_units: 2,
    },
    IsoCurrency {
        code: "ZWL",
        name: "Zimbabwean Dollar",
        minor_units: 2,
    },
];

/// The currency with this code, whatever its case.
pub fn find(code: &str) -> Option<&'static IsoCurrency> {
    let wanted = code.trim().to_uppercase();
    ISO_4217.iter().find(|one| one.code == wanted)
}

/// The whole catalogue, as the phone reads it.
pub fn catalogue() -> Value {
    json!({
        "currencies": ISO_4217
            .iter()
            .map(|one| json!({ "code": one.code, "name": one.name, "decimal_places": one.minor_units }))
            .collect::<Vec<_>>(),
        "count": ISO_4217.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_currency_the_world_uses_is_here_and_says_its_decimal_places() {
        // the ones that are not two places are the ones a Book gets wrong when it assumes
        assert_eq!(find("jpy").map(|one| one.minor_units), Some(0));
        assert_eq!(find("KWD").map(|one| one.minor_units), Some(3));
        assert_eq!(find("BHD").map(|one| one.minor_units), Some(3));
        assert_eq!(find("USD").map(|one| one.minor_units), Some(2));
        assert_eq!(find("ZWL").map(|one| one.minor_units), Some(2));
        assert!(find("ZZZ").is_none(), "a code the world does not use");
        assert!(
            ISO_4217.len() > 150,
            "the catalogue covers the world: {}",
            ISO_4217.len()
        );
        // sorted, so a reader can scan it
        let mut codes: Vec<&str> = ISO_4217.iter().map(|one| one.code).collect();
        codes.sort_unstable();
        assert_eq!(
            codes,
            ISO_4217.iter().map(|one| one.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_catalogue_reads_as_the_phone_needs_it() {
        let held = catalogue();
        assert_eq!(held["count"], json!(ISO_4217.len()));
        let yen = held["currencies"]
            .as_array()
            .expect("a list")
            .iter()
            .find(|one| one["code"] == json!("JPY"))
            .cloned()
            .expect("the yen is in the catalogue");
        assert_eq!(yen["decimal_places"], json!(0));
    }
}
