use warp::{self, path, Filter};
use tokio;
use futures;
use serde_derive;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::clone::Clone;

#[derive(Debug, serde_derive::Deserialize, serde_derive::Serialize)]
struct ServerJsonBody {
    port: u16,
}

struct RunningServer {
    shutdown: futures::sync::oneshot::Sender<()>
}

type Database = Arc<Mutex<HashMap<u16, RunningServer>>>;

fn list_servers(
    database: Database
) -> impl warp::Reply {
    let server_map = database.lock().unwrap();

    let keys: Vec<String> = server_map.keys()
        .map(|key| key.to_string())
        .collect();

    warp::reply::json(&keys)
}

fn post_new_server(
    database: Database,
    body: ServerJsonBody
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut server_map = database.lock().unwrap();

    if server_map.contains_key(&body.port) {
        return Err(warp::reject::not_found());
    }

    let (future, server) = create_warp_server(database.clone(), body.port);

    server_map.insert(body.port, server);

    tokio::spawn(future);
    Ok(warp::reply::json(&body))
}

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

fn create_warp_server(
    database: Database,
    port: u16
) -> (impl futures::future::Future<Item = (), Error = ()>, RunningServer) {
    let db_state = warp::any().map(move || database.clone());

    // `GET /` - list mock servers
    let get = warp::get2()
        .and(warp::path::end())
        .and(db_state.clone())
        .map(list_servers);

    // `POST /` - start mock server
    let post = warp::post2()
        .and(warp::path::end())
        .and(db_state.clone())
        .and(warp::body::json())
        .and_then(post_new_server);

    // 'DELETE /{port}' - delete mock server
    let delete = warp::delete2()
        .and(db_state)
        .and(path!(u16))
        .and_then(delete_server);

    let api = get.or(post).or(delete);

    let (tx, rx) = futures::sync::oneshot::channel();

    let (_, future) = warp::serve(api)
        .bind_with_graceful_shutdown(([127, 0, 0, 1], port), rx);

    (future, RunningServer{ shutdown: tx })
}

fn main() {
    let port = 8080;
    let db = Arc::new(Mutex::new(HashMap::new()));
    let (future, server) = create_warp_server(db.clone(), port);

    {
        let mut server_map = db.lock().unwrap();
        server_map.insert(port, server);
    }

    tokio::run(future);
}
