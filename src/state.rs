use actix_web::web::Data;
use std::collections::HashMap;
use std::sync::Mutex;

// State is just a hashmap
pub type State<T> = HashMap<String, T>;
pub type AppState<T> = Data<Mutex<State<T>>>;

/// Create a new state instance and wrap in a mutex.
/// Further wrap into an Actix Data instance.
pub fn new_state<T>() -> AppState<T> {
    let state = State::<T>::new();
    Data::new(Mutex::new(state))
}

/// Sets an entry in the application state by key.
/// Returns Some(T) only if the entry exists (update operation).
/// Returns None if the entry did not alreay exist (insert operation).
#[allow(dead_code)]
pub fn set<T>(data: AppState<T>, key: String, value: T) -> Option<T> {
    let mut hashmap = data.lock().expect("Could not acquire lock");
    hashmap.insert(key, value)
}

/// Get a copy of an application state entry by key.
/// Returns Some(T) only if the entry exists.
#[allow(dead_code)]
pub fn get<T>(data: AppState<T>, key: String) -> Option<T>
where
    T: Clone,
{
    let hashmap = data.lock().expect("Could not acquire lock");
    Some(hashmap.get(&key)?.to_owned())
}

/// Get a copy of an application state entry by key.
/// Returns Some(T) only if the entry exists.
#[allow(dead_code)]
pub fn all<T>(data: AppState<T>) -> Option<HashMap<String, T>>
where
    T: Clone,
{
    let hashmap = data.lock().expect("Could not acquire lock");
    Some(hashmap.clone())
}

/// Removes an entry in the application state by key.
/// Returns Some(T) only if the entry existed before removal.
#[allow(dead_code)]
pub fn delete<T>(data: AppState<T>, key: String) -> Option<T> {
    let mut hashmap = data.lock().expect("Could not acquire lock");
    hashmap.remove(&key)
}
