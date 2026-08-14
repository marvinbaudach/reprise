use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketAvailability {
    OnSale,
    OffSale,
    #[default]
    Unknown,
}

impl TicketAvailability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnSale => "on_sale",
            Self::OffSale => "off_sale",
            Self::Unknown => "unknown",
        }
    }
}

impl FromStr for TicketAvailability {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "on_sale" => Ok(Self::OnSale),
            "off_sale" => Ok(Self::OffSale),
            _ => Ok(Self::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::TicketAvailability;

    #[test]
    fn ticket_availability_roundtrips_through_persisted_text() {
        for availability in [
            TicketAvailability::OnSale,
            TicketAvailability::OffSale,
            TicketAvailability::Unknown,
        ] {
            assert_eq!(
                TicketAvailability::from_str(availability.as_str()).unwrap(),
                availability
            );
        }
    }

    #[test]
    fn conc_12_offsale_never_becomes_sold_out() {
        assert_eq!(
            TicketAvailability::from_str("offsale").unwrap(),
            TicketAvailability::Unknown
        );
        assert_eq!(
            TicketAvailability::from_str("off_sale").unwrap(),
            TicketAvailability::OffSale
        );
        assert_eq!(
            TicketAvailability::from_str("sold_out").unwrap(),
            TicketAvailability::Unknown
        );
    }
}
