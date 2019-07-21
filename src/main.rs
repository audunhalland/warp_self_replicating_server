use warp::{self, path, Filter};
use tokio;
use futures;
use serde_derive;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::clone::Clone;

/// JSON representation of a server instance
#[derive(Debug, serde_derive::Deserialize, serde_derive::Serialize)]
struct ServerJsonBody {
    port: u16,
}

/// In memory representation of a running server.
struct RunningServer {
    // Signal for shutting down the server
    shutdown: futures::sync::oneshot::Sender<()>
}

/// The application state / "Database". Each running server is keyed by its listening port.
type Database = Arc<Mutex<HashMap<u16, RunningServer>>>;

/// List all running servers
fn list_servers(
    database: Database
) -> impl warp::Reply {
    let server_map = database.lock().unwrap();

    let keys: Vec<ServerJsonBody> = server_map.keys()
        .map(|key| ServerJsonBody { port: *key })
        .collect();

    warp::reply::json(&keys)
}

/// Create a new server described by ServerJsonBody
fn post_new_server(
    database: Database,
    body: ServerJsonBody
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut server_map = database.lock().unwrap();

    if server_map.contains_key(&body.port) {
        return Err(warp::reject::not_found());
    }

    let (server, future) = create_warp_server(database.clone(), body.port);

    server_map.insert(body.port, server);

    tokio::spawn(future);
    Ok(warp::reply::json(&body))
}

/// Kill a server by port
fn delete_server(
    database: Database,
    port: u16
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut server_map = database.lock().unwrap();

    match server_map.remove(&port) {
        Some(server) => {
            server.shutdown.send(()).unwrap();
            Ok(warp::http::StatusCode::NO_CONTENT)
        }
        None => Err(warp::reject::not_found())
    }
}

/// Create a warp filter representing the app's HTTP routes and handlers
fn app_filter(
    database: Database
) -> warp::filters::BoxedFilter<(impl warp::reply::Reply,)> {
    let db_arg = warp::any().map(move || database.clone());

    // `GET /` - list mock servers
    let get = db_arg.clone()
        .and(warp::get2())
        .and(warp::path::end())
        .map(list_servers);

    // `POST /` - start mock server
    let post = db_arg.clone()
        .and(warp::post2())
        .and(warp::path::end())
        .and(warp::body::json())
        .and_then(post_new_server);

    // 'DELETE /{port}' - delete mock server
    let delete = db_arg.clone()
        .and(warp::delete2())
        .and(path!(u16))
        .and_then(delete_server);

    get.or(post).or(delete).boxed()
}

// Create an instance of HTTP server
fn create_warp_server(
    database: Database,
    port: u16
) -> (RunningServer, impl futures::future::Future<Item = (), Error = ()>) {
    let (tx, rx) = futures::sync::oneshot::channel();

    let (_, future) = warp::serve(app_filter(database))
        .bind_with_graceful_shutdown(([127, 0, 0, 1], port), rx);

    (RunningServer{ shutdown: tx }, future)
}

fn main() {
    let port = 8080;
    let database = Arc::new(Mutex::new(HashMap::new()));
    let (server, future) = create_warp_server(database.clone(), port);

    {
        let mut server_map = database.lock().unwrap();
        server_map.insert(port, server);
    }

    tokio::run(future);
}
