use godot::builtin::{VarArray, VarDictionary};
use godot::prelude::*;

use kasane_core::types::{Status, Vec2};

pub type Dictionary = VarDictionary;
pub type Array = VarArray;

pub fn is_main_thread() -> bool {
    let os = godot::classes::Os::singleton();
    os.get_thread_caller_id() == os.get_main_thread_id()
}

pub fn status_to_dict(status: &Status) -> Dictionary {
    let mut dictionary = Dictionary::new();
    dictionary.set("ok", status.is_ok());
    dictionary.set("code", status.code.as_str());
    dictionary.set("message", status.message.as_str());
    dictionary
}

pub fn error_dict(code: &str, message: &str) -> Dictionary {
    status_to_dict(&Status::error(code, message))
}

pub fn vectors_to_packed(vectors: &[Vec2]) -> PackedVector2Array {
    let mut packed = PackedVector2Array::new();
    packed.resize(vectors.len());
    for (index, vector) in vectors.iter().enumerate() {
        packed[index] = Vector2::new(vector.x, vector.y);
    }
    packed
}

pub fn packed_to_vectors(packed: &PackedVector2Array) -> Vec<Vec2> {
    let mut vectors = Vec::with_capacity(packed.len());
    for index in 0..packed.len() {
        let vector = packed[index];
        vectors.push(Vec2::new(vector.x, vector.y));
    }
    vectors
}
