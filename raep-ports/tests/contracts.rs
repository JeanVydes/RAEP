use raep_domain::{DomainEvent, DomainEventKind, EntityId, Session};
use raep_ports::{
    BoxFuture, Clock, ModelEvent, ModelProvider, ModelRequest, ModelStream, SessionStore,
    ToolExecutorPort, ToolRequest, ToolResult,
};
use raep_types::ToolDescriptor;
use std::convert::Infallible;

struct TestStore(Option<Session>);

impl SessionStore for TestStore {
    type Error = Infallible;

    fn load(&self, id: &EntityId) -> Result<Option<Session>, Self::Error> {
        Ok(self
            .0
            .as_ref()
            .filter(|session| session.id() == id)
            .cloned())
    }

    fn commit(
        &mut self,
        session: Session,
        events: Vec<DomainEvent>,
    ) -> Result<Vec<raep_ports::RecordedEvent>, Self::Error> {
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind(),
            DomainEventKind::SessionCreated { .. }
        ));
        self.0 = Some(session);
        Ok(Vec::new())
    }

    fn events_after(
        &self,
        _id: &EntityId,
        _sequence: u64,
        _limit: usize,
    ) -> Result<Vec<raep_ports::RecordedEvent>, Self::Error> {
        Ok(Vec::new())
    }
}

struct TestClock;

impl Clock for TestClock {
    fn now_millis(&self) -> i64 {
        123
    }
}

#[test]
fn ports_are_implementable_without_framework_types() {
    let id = EntityId::new("session").unwrap();
    let mut store = TestStore(None);
    let clock = TestClock;
    let event = DomainEvent::new(
        clock.now_millis(),
        DomainEventKind::SessionCreated {
            session_id: id.clone(),
        },
    );
    store.commit(Session::new(id.clone()), vec![event]).unwrap();
    assert!(store.load(&id).unwrap().is_some());
}

struct EmptyModelStream;

impl ModelStream for EmptyModelStream {
    type Error = Infallible;

    fn next<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<ModelEvent>, Self::Error>> {
        Box::pin(async { Ok(None) })
    }
}

struct TestModel;

impl ModelProvider for TestModel {
    type Error = Infallible;
    type Stream = EmptyModelStream;

    fn generate<'a>(
        &'a mut self,
        _request: ModelRequest,
    ) -> BoxFuture<'a, Result<Self::Stream, Self::Error>> {
        Box::pin(async { Ok(EmptyModelStream) })
    }
}

struct TestTools;

impl ToolExecutorPort for TestTools {
    type Error = Infallible;

    fn execute<'a>(
        &'a mut self,
        request: ToolRequest,
    ) -> BoxFuture<'a, Result<ToolResult, Self::Error>> {
        Box::pin(async move {
            Ok(ToolResult {
                tool_call_id: request.tool_call_id,
                succeeded: true,
                content: serde_json::json!({}),
                usage: None,
            })
        })
    }
}

#[test]
fn execution_ports_are_runtime_agnostic() {
    let mut model = TestModel;
    let mut tools = TestTools;
    let request = ModelRequest {
        model: "example".to_owned(),
        messages: Vec::new(),
        tools: vec![ToolDescriptor {
            name: "echo".to_owned(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            executor: None,
            risk: None,
            side_effects: None,
            idempotent: Some(true),
            timeout_s: None,
            scopes: Vec::new(),
            detachable: None,
        }],
    };
    let mut stream = futures_lite_block_on(model.generate(request)).unwrap();
    assert!(futures_lite_block_on(stream.next()).unwrap().is_none());
    let result = futures_lite_block_on(tools.execute(ToolRequest {
        descriptor: ToolDescriptor {
            name: "echo".to_owned(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            executor: None,
            risk: None,
            side_effects: None,
            idempotent: Some(true),
            timeout_s: None,
            scopes: Vec::new(),
            detachable: None,
        },
        tool_call_id: raep_domain::EntityId::new("call").unwrap(),
        arguments: serde_json::json!({}),
    }))
    .unwrap();
    assert!(result.succeeded);
}

fn futures_lite_block_on<T>(future: BoxFuture<'_, T>) -> T {
    futures_lite::future::block_on(future)
}
