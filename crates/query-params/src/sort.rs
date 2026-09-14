use serde::Deserialize;

#[doc = include_str!("../docs/sort.SortOrder.md")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[doc = include_str!("../docs/sort.SortOrder.Asc.md")]
    Asc,
    #[doc = include_str!("../docs/sort.SortOrder.Desc.md")]
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Asc
    }
}

impl SortOrder {
    #[doc = include_str!("../docs/sort.SortOrder.from_str.md")]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "asc" | "ascending" => Some(SortOrder::Asc),
            "desc" | "descending" => Some(SortOrder::Desc),
            _ => None,
        }
    }
}

#[doc = include_str!("../docs/sort.Sort.md")]
#[derive(Debug, Clone, PartialEq)]
pub struct Sort {
    #[doc = include_str!("../docs/sort.Sort.field.md")]
    pub field: String,
    #[doc = include_str!("../docs/sort.Sort.order.md")]
    pub order: SortOrder,
}

impl Sort {
    #[doc = include_str!("../docs/sort.Sort.new.md")]
    pub fn new(field: impl Into<String>, order: SortOrder) -> Self {
        Self {
            field: field.into(),
            order,
        }
    }

    #[doc = include_str!("../docs/sort.Sort.from_params.md")]
    pub fn from_params(sort_field: Option<String>, order_str: Option<String>) -> Option<Self> {
        sort_field.map(|field| {
            let order = order_str
                .and_then(|s| SortOrder::from_str(&s))
                .unwrap_or_default();
            Sort { field, order }
        })
    }

    #[doc = include_str!("../docs/sort.Sort.is_ascending.md")]
    pub fn is_ascending(&self) -> bool {
        matches!(self.order, SortOrder::Asc)
    }

    #[doc = include_str!("../docs/sort.Sort.is_descending.md")]
    pub fn is_descending(&self) -> bool {
        matches!(self.order, SortOrder::Desc)
    }
}
