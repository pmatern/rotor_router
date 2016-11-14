extern crate rotor;
extern crate rotor_http;
extern crate num_cpus;

use std::time::Duration;

use rotor::{Loop, LoopInstance, Scope, Time};
use rotor_http::server::{RecvMode, Server, Head, Response, Fsm};
use rotor::mio::tcp::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ServerContext {
    counter: AtomicUsize,
}

trait Counter {
    fn increment(& self);
    fn get(&self) -> usize;
}

impl Counter for ServerContext {
    fn increment(& self) {
        let _ = self.counter.fetch_add(1, Ordering::SeqCst);
    }
    fn get(&self) -> usize { 
        self.counter.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
struct Downstream {
}

#[derive(Debug, Clone)]
enum Router {
    ForwardingRequest(Downstream),
    ForwardingResponse(Downstream),
    NoRoute,
}

fn stream_chunk(_res: &mut Response, _data: &[u8], _downstream: &Downstream) {
   
}


fn send_string(res: &mut Response, data: &[u8]) {
    res.status(200, "OK");
    res.add_length(data.len() as u64).unwrap();
    res.done_headers().unwrap();
    res.write_body(data);
    res.done();
}

fn lookup_route(head: Head) -> Option<Downstream> {
    match head.path {
        "/downstream" => Some(Downstream{}),
        _ => None
    }
}

impl Server for Router {
    type Seed = ();
    type Context = Arc<ServerContext>;

    fn headers_received(_seed: (), head: Head, _response: &mut Response, scope: &mut Scope<Arc<ServerContext>>) -> Option<(Self, RecvMode, Time)>
    {
        scope.increment();

        let next_machine = match lookup_route(head) {
            Some(destination) => Router::ForwardingRequest(destination),
            None => Router::NoRoute
        };

        Some((next_machine, RecvMode::Progressive(1024), scope.now() + Duration::from_secs(10)))
    } 

    fn request_received(self, _data: &[u8], _response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        unreachable!();
    }

    fn request_chunk(self, chunk: &[u8], response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        let next_machine = match self {
            Router::ForwardingRequest(downstream) => {
                stream_chunk(response, chunk, &downstream);
                Router::ForwardingRequest(downstream)
            }
            Router::NoRoute => {
                self
            }
            _ => self
        };

        match next_machine {
            Router::ForwardingRequest(_) => Some(next_machine),
            Router::NoRoute => Some(next_machine),
            _ => None
        }
    }

    fn request_end(self, response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        let next_machine = match self {
            Router::ForwardingRequest(downstream) => {
                send_string(response, b"Response from downstream");
                Router::ForwardingResponse(downstream)
            }
            Router::NoRoute => {
                let data = b"404 - Route not found";
                response.status(404, "Not Found");
                response.add_length(data.len() as u64).unwrap();
                response.done_headers().unwrap();
                response.write_body(data);
                response.done();
                self
            }
            _ => self
        };

        match next_machine {
            Router::ForwardingResponse(_) => Some(next_machine),
            _ => None
        }
    }

    fn timeout(self, _response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<(Self, Time)>
    {
        unimplemented!();
    }

    fn wakeup(self, _response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        unimplemented!();
    }
}

fn init_worker <'a> (listener: TcpListener, context: Arc<ServerContext>) -> LoopInstance<Fsm<Router, TcpListener>> {
    let event_loop = Loop::new(&rotor::Config::new()).unwrap();

    let mut loop_inst= event_loop.instantiate(context);

    loop_inst.add_machine_with(|scope| {
        Fsm::<Router, _>::new(listener, (), scope)
    }).unwrap();

    loop_inst
}

fn main() {
    println!("Starting router on http://127.0.0.1:3000/");

    let mut workers = Vec::new();
    let lst = TcpListener::bind(&"127.0.0.1:3000".parse().unwrap()).unwrap();
    let context = Arc::new(ServerContext { counter: AtomicUsize::new(0), });

    for _ in 0..num_cpus::get() {
        let listener = lst.try_clone().unwrap();
        let context = context.clone();

        workers.push(std::thread::spawn(move || {
            init_worker(listener, context).run().unwrap();
        }));         
    }

    for worker in workers {
        worker.join().unwrap();
    }

}
