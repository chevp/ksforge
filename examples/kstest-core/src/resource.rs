#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceRequest {
    Connect,
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceResult {
    Connected,
    Disconnected,
    Unavailable(String),
}

/// Something external a capability might need: a socket, an HTTP service, a
/// GPU, a filesystem, a database, an external process, a device, ... The
/// core only ever sees this generic shape — never which kind of resource
/// it is, and it carries no state-machine logic of its own.
pub trait Resource {
    fn id(&self) -> ResourceId;
    fn execute(&self, request: ResourceRequest) -> ResourceResult;
}

/// Deterministic stand-in resource for the example/tests. Always connects
/// successfully; a real resource would report `Unavailable` when it can't
/// reach whatever it wraps.
pub struct MockResource;

impl Resource for MockResource {
    fn id(&self) -> ResourceId {
        ResourceId("primary")
    }

    fn execute(&self, request: ResourceRequest) -> ResourceResult {
        match request {
            ResourceRequest::Connect => ResourceResult::Connected,
            ResourceRequest::Disconnect => ResourceResult::Disconnected,
        }
    }
}
