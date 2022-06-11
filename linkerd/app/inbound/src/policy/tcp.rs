use super::{AllowPolicy, ServerPermit, ServerUnauthorized};
use crate::metrics::authz::TcpAuthzMetrics;
use futures::future;
use linkerd_app_core::{
    svc, tls,
    transport::{ClientAddr, Remote},
    Error, Result,
};
use std::{future::Future, pin::Pin, task};

/// A middleware that enforces policy on each TCP connection. When connection is authorized, we
/// continue to monitor the policy for changes and, if the connection is no longer authorized, it is
/// dropped/closed.
///
/// Metrics are reported to the `TcpAuthzMetrics` struct.
#[derive(Clone, Debug)]
pub struct NewTcpPolicy<N> {
    inner: N,
    metrics: TcpAuthzMetrics,
}

#[derive(Clone, Debug)]
pub enum AuthorizeTcp<S> {
    Authorized(Authorized<S>),
    Unauthorized(ServerUnauthorized),
}

#[derive(Clone, Debug)]
pub struct Authorized<S> {
    inner: S,
    policy: AllowPolicy,
    client: Remote<ClientAddr>,
    tls: tls::ConditionalServerTls,
    metrics: TcpAuthzMetrics,
}

// === impl NewTcpPolicy ===

impl<N> NewTcpPolicy<N> {
    pub(crate) fn layer(
        metrics: TcpAuthzMetrics,
    ) -> impl svc::layer::Layer<N, Service = Self> + Clone {
        svc::layer::mk(move |inner| Self {
            inner,
            metrics: metrics.clone(),
        })
    }
}

impl<T, N> svc::NewService<T> for NewTcpPolicy<N>
where
    T: svc::Param<AllowPolicy>
        + svc::Param<Remote<ClientAddr>>
        + svc::Param<tls::ConditionalServerTls>,
    N: svc::NewService<(ServerPermit, T)>,
{
    type Service = AuthorizeTcp<N::Service>;

    fn new_service(&self, target: T) -> Self::Service {
        let client = target.param();
        let tls = target.param();
        let policy: AllowPolicy = target.param();
        tracing::trace!(?policy, "Authorizing connection");
        match policy.check_authorized(client, &tls) {
            Ok(permit) => {
                tracing::debug!(?permit, ?tls, %client, "Connection authorized");

                // This new services requires a ClientAddr, so it must necessarily be built for each
                // connection. So we can just increment the counter here since the service can only
                // be used at most once.
                self.metrics.allow(&permit, tls.clone());

                let inner = self.inner.new_service((permit, target));
                AuthorizeTcp::Authorized(Authorized {
                    inner,
                    policy,
                    client,
                    tls,
                    metrics: self.metrics.clone(),
                })
            }
            Err(deny) => {
                tracing::info!(
                    server.group = %policy.group(),
                    server.kind = %policy.kind(),
                    server.name = %policy.name(),
                    ?tls, %client,
                    "Connection denied"
                );
                self.metrics.deny(&policy, tls);
                AuthorizeTcp::Unauthorized(deny)
            }
        }
    }
}

// === impl AuthorizeTcp ===

impl<I, S> svc::Service<I> for AuthorizeTcp<S>
where
    S: svc::Service<I, Response = ()>,
    S::Error: Into<Error>,
    S::Future: Send + 'static,
{
    type Response = ();
    type Error = Error;
    type Future = future::Either<
        Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>,
        future::Ready<Result<()>>,
    >;

    #[inline]
    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> task::Poll<Result<()>> {
        match self {
            Self::Authorized(Authorized { ref mut inner, .. }) => {
                inner.poll_ready(cx).map_err(Into::into)
            }

            // If connections are not authorized, fail it immediately.
            Self::Unauthorized(deny) => task::Poll::Ready(Err(deny.clone().into())),
        }
    }

    fn call(&mut self, io: I) -> Self::Future {
        let Authorized {
            inner,
            client,
            tls,
            policy,
            metrics,
        } = match self {
            Self::Authorized(a) => a,
            Self::Unauthorized(_deny) => unreachable!("poll_ready must be called"),
        };

        // If the connection is authorized, pass it to the inner service and stop processing the
        // connection if the authorization's state changes to no longer permit the request.
        let client = *client;
        let tls = tls.clone();
        let mut policy = policy.clone();
        let metrics = metrics.clone();

        let call = inner.call(io);
        future::Either::Left(Box::pin(async move {
            tokio::pin!(call);
            loop {
                tokio::select! {
                    res = &mut call => return res.map_err(Into::into),
                    _ = policy.changed() => {
                        if let Err(denied) = policy.check_authorized(client, &tls) {
                            tracing::info!(
                                server.group = %policy.group(),
                                server.kind = %policy.kind(),
                                server.name = %policy.name(),
                                ?tls,
                                %client,
                                "Connection terminated due to policy change",
                            );
                            metrics.terminate(&policy, tls);
                            return Err(denied.into());
                        }
                    }
                };
            }
        }))
    }
}