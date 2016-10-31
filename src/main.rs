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
enum HelloWorld {
    Hello,
    GetNum,
    HelloName(String),
    PageNotFound,
}

fn send_string(res: &mut Response, data: &[u8]) {
    res.status(200, "OK");
    res.add_length(data.len() as u64).unwrap();
    res.done_headers().unwrap();
    res.write_body(data);
    res.done();
}

impl Server for HelloWorld {
    type Seed = ();
    type Context = Arc<ServerContext>;

    fn headers_received(_seed: (), head: Head, _res: &mut Response, scope: &mut Scope<Arc<ServerContext>>) -> Option<(Self, RecvMode, Time)>
    {
        scope.increment();

        let next_machine = match head.path {
            "/" => HelloWorld::Hello,
            "/num" => HelloWorld::GetNum,
            p if p.starts_with('/') => HelloWorld::HelloName(p[1..].to_string()),
            _ => HelloWorld::PageNotFound
        };

        Some((next_machine, RecvMode::Buffered(1024), scope.now() + Duration::from_secs(10)))
    } 

    fn request_received(self, _data: &[u8], res: &mut Response, scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        match self {
            HelloWorld::Hello => {
                send_string(res, b"Hello World!\n");
            }
            HelloWorld::GetNum => {
                send_string(res,
                    format!("This host has been visited {} times\n", scope.get()).as_bytes());
            }
            HelloWorld::HelloName(name) => {
                send_string(res, format!("Hello {}!\n", name).as_bytes());
            }
            HelloWorld::PageNotFound => {
                let data = b"404 - Page not found";
                res.status(404, "Not Found");
                res.add_length(data.len() as u64).unwrap();
                res.done_headers().unwrap();
                res.write_body(data);
                res.done();
            }
        }
        None
    }

    fn request_chunk(self, _chunk: &[u8], _response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        unreachable!();
    }

    fn request_end(self, _response: &mut Response, _scope: &mut Scope<Arc<ServerContext>>) -> Option<Self>
    {
        unreachable!();
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

fn init_worker (listener: TcpListener, context: Arc<ServerContext>) -> LoopInstance<Fsm<HelloWorld, TcpListener>> {
    let event_loop = Loop::new(&rotor::Config::new()).unwrap();

    let mut loop_inst= event_loop.instantiate(context);

    loop_inst.add_machine_with(|scope| {
        Fsm::<HelloWorld, _>::new(listener, (), scope)
    }).unwrap();

    loop_inst
}

fn main() {
    println!("Starting internal router on http://127.0.0.1:3000/");

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
