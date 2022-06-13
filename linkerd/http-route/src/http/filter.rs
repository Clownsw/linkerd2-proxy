mod error_respond;
mod modify_request_header;
mod redirect;

pub use self::{
    error_respond::RespondWithError,
    modify_request_header::ModifyRequestHeader,
    redirect::{InvalidRedirect, RedirectRequest, Redirection},
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ModifyPath {
    ReplaceFullPath(String),
    ReplacePrefixMatch(String),
}
