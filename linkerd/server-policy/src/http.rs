use linkerd_http_route::http;
pub use linkerd_http_route::http::{filter, r#match, RouteMatch};

pub type Policy = crate::RoutePolicy<Filter>;
pub type Route = http::Route<Policy>;
pub type Rule = http::Rule<Policy>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    Error(http::filter::RespondWithError),

    RequestHeaders(http::filter::ModifyRequestHeader),

    Redirect(http::filter::RedirectRequest),

    /// Indicates that the filter kind is unknown to the proxy (e.g., because
    /// the controller is on a new version of the protobuf).
    ///
    /// Route handlers must be careful about this situation, as it may not be
    /// appropriate for a proxy to skip filtering logic.
    Unknown,
}

#[inline]
pub fn find<'r, B>(
    routes: &'r [Route],
    req: &::http::Request<B>,
) -> Option<(http::RouteMatch, &'r Policy)> {
    http::find(routes, req)
}

pub fn default(authorizations: std::sync::Arc<[crate::Authorization]>) -> Route {
    Route {
        hosts: vec![],
        rules: vec![Rule {
            matches: vec![],
            policy: Policy {
                meta: crate::Meta::new_default("default"),
                authorizations,
                filters: vec![],
            },
        }],
    }
}