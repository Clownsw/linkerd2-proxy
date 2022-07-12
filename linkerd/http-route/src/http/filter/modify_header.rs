use http::header::{HeaderMap, HeaderName, HeaderValue};

// Note that while `set` and `remove` could perhaps be better-modled as a Map
// and Set, we use a vector here so that they can implement `Hash`, etc. We
// can't use a `BTree*` because `HeaderName` does not implement `Ord`.
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub struct ModifyHeader {
    pub add: Vec<(HeaderName, HeaderValue)>,
    pub set: Vec<(HeaderName, HeaderValue)>,
    pub remove: Vec<HeaderName>,
}

// === impl ModifyRequestHeader ===

impl ModifyHeader {
    pub fn apply(&self, headers: &mut HeaderMap) {
        for (hdr, val) in &self.add {
            headers.append(hdr, val.clone());
        }
        for (hdr, val) in &self.set {
            headers.insert(hdr, val.clone());
        }
        for hdr in &self.remove {
            headers.remove(hdr);
        }
    }
}

#[cfg(feature = "proto")]
pub mod proto {
    use super::*;
    use http::header::{InvalidHeaderName, InvalidHeaderValue};
    use linkerd2_proxy_api::{http_route as api, http_types};
    use std::sync::Arc;

    #[derive(Clone, Debug, thiserror::Error)]
    pub enum InvalidModifyHeader {
        #[error("{0}")]
        Name(#[source] Arc<InvalidHeaderName>),

        #[error("{0}")]
        Value(#[source] Arc<InvalidHeaderValue>),
    }

    // === impl ModifyRequestHeader ===

    impl TryFrom<api::RequestHeaderModifier> for ModifyHeader {
        type Error = InvalidModifyHeader;

        fn try_from(rhm: api::RequestHeaderModifier) -> Result<Self, Self::Error> {
            fn to_pairs(
                hs: Option<http_types::Headers>,
            ) -> Result<Vec<(HeaderName, HeaderValue)>, InvalidModifyHeader> {
                hs.into_iter()
                    .flat_map(|a| a.headers.into_iter())
                    .map(|h| {
                        let name = h
                            .name
                            .parse::<HeaderName>()
                            .map_err(|e| InvalidModifyHeader::Name(e.into()))?;
                        let value = HeaderValue::from_bytes(&h.value)
                            .map_err(|e| InvalidModifyHeader::Value(e.into()))?;
                        Ok((name, value))
                    })
                    .collect()
            }

            let add = to_pairs(rhm.add)?;
            let set = to_pairs(rhm.set)?;
            let remove = rhm
                .remove
                .into_iter()
                .map(|n| {
                    n.parse::<HeaderName>()
                        .map_err(|e| InvalidModifyHeader::Name(e.into()))
                })
                .collect::<Result<Vec<HeaderName>, InvalidModifyHeader>>()?;
            Ok(ModifyHeader { add, set, remove })
        }
    }
}
