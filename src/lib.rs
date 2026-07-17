pub mod recipes;

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use recipes::tags::Tags;
use recipes::tree_builder::{prepare_index, PreparedIndex, RawRecipe, RawSlot};
use recipes::Item;

// ---------------------------------------------------------------------------
// C ABI for Java FFI (java.lang.foreign).
//
// Java already holds recipes, tags, and inventory in memory, so nothing is
// serialized: Java walks its registries once, pushing each tag and recipe
// across as plain C strings, then calls `ccmk4_build` per inventory. The
// expensive preparation (tag expansion, id interning, slot consolidation) runs
// once on first build and is cached inside the handle until the data changes.
//
// Handles are NOT thread-safe; guard each builder with a lock (or confine it to
// one thread) on the Java side. Results are independent immutable objects.
// ---------------------------------------------------------------------------

thread_local! {
    /// Message of the most recent failure on this thread, exposed via
    /// [`ccmk4_last_error`]. FFI callers cannot receive Rust errors directly, so
    /// a failing call returns null/false and stores the reason here.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(message: String) {
    let message = CString::new(message.replace('\0', "?"))
        .unwrap_or_else(|_| CString::new("unknown error").unwrap());
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(message));
}

/// Runs `f`, converting errors and panics into `fallback` + a stored message. A
/// panic unwinding out of an `extern "C"` fn aborts the process — fatal to a
/// host JVM — so every entry point goes through this.
fn guarded<T>(fallback: T, f: impl FnOnce() -> Result<T, String>) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value)) => value,
        Ok(Err(message)) => {
            set_last_error(message);
            fallback
        }
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic with non-string payload".to_string());
            set_last_error(format!("internal panic: {message}"));
            fallback
        }
    }
}

/// Reads a caller-supplied C string argument, rejecting null and invalid UTF-8.
///
/// # Safety
/// `ptr` must be null or point to a valid null-terminated string that outlives
/// this call.
unsafe fn read_str<'a>(ptr: *const c_char, name: &str) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err(format!("{name} pointer is null"));
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| format!("{name} is not valid UTF-8: {e}"))
}

/// Reads an array of C strings.
///
/// # Safety
/// `ptr` must point to `len` valid C string pointers (or be null when `len` is 0),
/// all valid for the duration of the call.
unsafe fn read_str_array<'a>(
    ptr: *const *const c_char,
    len: usize,
    name: &str,
) -> Result<Vec<&'a str>, String> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if ptr.is_null() {
        return Err(format!("{name} pointer is null but length is {len}"));
    }
    unsafe { std::slice::from_raw_parts(ptr, len) }
        .iter()
        .enumerate()
        .map(|(i, &p)| unsafe { read_str(p, &format!("{name}[{i}]")) })
        .collect()
}

/// # Safety
/// `ptr` must be null or a live pointer previously returned by `ccmk4_builder_new`.
unsafe fn builder_arg<'a>(ptr: *mut Ccmk4Builder) -> Result<&'a mut Ccmk4Builder, String> {
    if ptr.is_null() {
        return Err("builder pointer is null".to_string());
    }
    Ok(unsafe { &mut *ptr })
}

struct OwnedRecipe {
    output: String,
    yield_count: u64,
    slots: Vec<Vec<String>>,
}

/// Opaque handle accumulating tags and recipes from the host, with the prepared
/// crafting index cached across builds.
pub struct Ccmk4Builder {
    tags: Tags,
    recipes: Vec<OwnedRecipe>,
    pending: Option<OwnedRecipe>,
    prepared: Option<PreparedIndex>,
}

/// One build result: item ids (as C strings, owned by the result) and quantities.
pub struct Ccmk4Result {
    entries: Vec<(CString, u32)>,
}

/// Creates an empty builder. Free it with [`ccmk4_builder_free`].
///
/// # Safety
/// Always safe to call; the returned handle must only be used from one thread at
/// a time and freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_builder_new() -> *mut Ccmk4Builder {
    Box::into_raw(Box::new(Ccmk4Builder {
        tags: Tags::default(),
        recipes: Vec::new(),
        pending: None,
        prepared: None,
    }))
}

/// Destroys a builder and everything it owns. Passing null is a safe no-op.
///
/// # Safety
/// `builder` must be null or a pointer from [`ccmk4_builder_new`] that has not
/// already been freed, with no other live references to it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_builder_free(builder: *mut Ccmk4Builder) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

/// Registers one item tag: `tag_id` is the full id (e.g. `minecraft:planks`),
/// `members` are `member_count` tokens, each an item id or a nested `#tag`
/// reference. Re-registering a tag replaces its members. Returns false on error
/// (see [`ccmk4_last_error`]).
///
/// # Safety
/// `builder` must be a live builder handle. `members` must point to
/// `member_count` valid null-terminated strings (strings are copied; they only
/// need to live for this call).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_builder_add_tag(
    builder: *mut Ccmk4Builder,
    tag_id: *const c_char,
    members: *const *const c_char,
    member_count: usize,
) -> bool {
    guarded(false, || {
        let builder = unsafe { builder_arg(builder) }?;
        let tag_id = unsafe { read_str(tag_id, "tag_id") }?;
        let members = unsafe { read_str_array(members, member_count, "members") }?;
        builder.tags.add(
            tag_id.to_string(),
            members.into_iter().map(str::to_string).collect(),
        );
        builder.prepared = None;
        Ok(true)
    })
}

/// Starts a new recipe with its output item id and per-craft yield (a yield of 0
/// is treated as 1). Add slots with [`ccmk4_recipe_slot`], then finish with
/// [`ccmk4_recipe_commit`]. Beginning a recipe while another is in progress
/// discards the unfinished one. Returns false on error.
///
/// # Safety
/// `builder` must be a live builder handle; `output_id` a valid null-terminated
/// string (copied during the call).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_recipe_begin(
    builder: *mut Ccmk4Builder,
    output_id: *const c_char,
    output_count: u32,
) -> bool {
    guarded(false, || {
        let builder = unsafe { builder_arg(builder) }?;
        let output = unsafe { read_str(output_id, "output_id") }?;
        builder.pending = Some(OwnedRecipe {
            output: output.to_string(),
            yield_count: if output_count == 0 {
                1
            } else {
                output_count as u64
            },
            slots: Vec::new(),
        });
        Ok(true)
    })
}

/// Adds one ingredient slot to the recipe in progress: `tokens` are the
/// `token_count` interchangeable options for the slot (item ids or `#tag`
/// references). Call once per occupied grid slot; identical slots are
/// consolidated automatically. Returns false on error.
///
/// # Safety
/// `builder` must be a live builder handle. `tokens` must point to `token_count`
/// valid null-terminated strings (copied during the call).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_recipe_slot(
    builder: *mut Ccmk4Builder,
    tokens: *const *const c_char,
    token_count: usize,
) -> bool {
    guarded(false, || {
        let builder = unsafe { builder_arg(builder) }?;
        let tokens = unsafe { read_str_array(tokens, token_count, "tokens") }?;
        let Some(pending) = builder.pending.as_mut() else {
            return Err("ccmk4_recipe_slot called without ccmk4_recipe_begin".to_string());
        };
        pending
            .slots
            .push(tokens.into_iter().map(str::to_string).collect());
        Ok(true)
    })
}

/// Finishes the recipe in progress and registers it. Returns false on error
/// (including when no recipe is in progress).
///
/// # Safety
/// `builder` must be a live builder handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_recipe_commit(builder: *mut Ccmk4Builder) -> bool {
    guarded(false, || {
        let builder = unsafe { builder_arg(builder) }?;
        let Some(recipe) = builder.pending.take() else {
            return Err("ccmk4_recipe_commit called without ccmk4_recipe_begin".to_string());
        };
        builder.recipes.push(recipe);
        builder.prepared = None;
        Ok(true)
    })
}

/// Computes the craftable items for one inventory, given as parallel arrays of
/// `count` item ids and quantities. The prepared crafting index is cached inside
/// the builder, so repeated builds with different inventories are cheap. Returns
/// a result handle to read with the `ccmk4_result_*` functions and free with
/// [`ccmk4_result_free`], or null on error (see [`ccmk4_last_error`]).
///
/// # Safety
/// `builder` must be a live builder handle with no recipe left uncommitted.
/// `item_ids` must point to `count` valid null-terminated strings and
/// `quantities` to `count` u32 values, all valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_build(
    builder: *mut Ccmk4Builder,
    item_ids: *const *const c_char,
    quantities: *const u32,
    count: usize,
) -> *mut Ccmk4Result {
    guarded(std::ptr::null_mut(), || {
        let builder = unsafe { builder_arg(builder) }?;
        let ids = unsafe { read_str_array(item_ids, count, "item_ids") }?;
        if count > 0 && quantities.is_null() {
            return Err(format!("quantities pointer is null but count is {count}"));
        }
        let quantities = if count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(quantities, count) }
        };
        let inventory: Vec<Item> = ids
            .iter()
            .zip(quantities)
            .map(|(id, &quantity)| Item {
                id: id.to_string(),
                quantity,
            })
            .collect();

        if builder.prepared.is_none() {
            builder.prepared = Some(prepare_index(
                builder.recipes.iter().map(|recipe| RawRecipe {
                    output: &recipe.output,
                    yield_count: recipe.yield_count,
                    slots: recipe
                        .slots
                        .iter()
                        .map(|tokens| RawSlot {
                            tokens: tokens.iter().map(String::as_str).collect(),
                            count: 1,
                        })
                        .collect(),
                }),
                &builder.tags,
            ));
        }
        let index = builder.prepared.as_ref().expect("prepared above");

        let entries = index
            .run(&inventory)
            .into_iter()
            .map(|item| {
                CString::new(item.id)
                    .map(|id| (id, item.quantity))
                    .map_err(|e| format!("item id contained a NUL byte: {e}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Box::into_raw(Box::new(Ccmk4Result { entries })))
    })
}

/// Number of entries in a build result. Returns 0 for null.
///
/// # Safety
/// `result` must be null or a live pointer from [`ccmk4_build`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_result_len(result: *const Ccmk4Result) -> usize {
    if result.is_null() {
        return 0;
    }
    unsafe { &*result }.entries.len()
}

/// Item id of entry `index`, or null when out of range. The string is owned by
/// the result and valid until [`ccmk4_result_free`].
///
/// # Safety
/// `result` must be null or a live pointer from [`ccmk4_build`]; the returned
/// string must not be written through, freed, or used after the result is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_result_id(result: *const Ccmk4Result, index: usize) -> *const c_char {
    if result.is_null() {
        return std::ptr::null();
    }
    unsafe { &*result }
        .entries
        .get(index)
        .map_or(std::ptr::null(), |(id, _)| id.as_ptr())
}

/// Craftable quantity of entry `index`, or 0 when out of range.
///
/// # Safety
/// `result` must be null or a live pointer from [`ccmk4_build`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_result_quantity(result: *const Ccmk4Result, index: usize) -> u32 {
    if result.is_null() {
        return 0;
    }
    unsafe { &*result }
        .entries
        .get(index)
        .map_or(0, |&(_, quantity)| quantity)
}

/// Destroys a build result. Passing null is a safe no-op.
///
/// # Safety
/// `result` must be null or a pointer from [`ccmk4_build`] that has not already
/// been freed; strings previously returned for it become invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_result_free(result: *mut Ccmk4Result) {
    if !result.is_null() {
        drop(unsafe { Box::from_raw(result) });
    }
}

/// Returns the error message of the most recent failed call on the current
/// thread, or null if no call has failed. The pointer is owned by the library
/// and is only valid until the next failing call from this thread — copy the
/// string before calling anything else; do not free it.
///
/// # Safety
/// The returned pointer must not be written through, freed, or dereferenced
/// after a subsequent failing call into this library from the same thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ccmk4_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |message| message.as_ptr())
    })
}

#[cfg(test)]
mod ffi_tests {
    use super::*;
    use recipes::recipe::Recipe;
    use recipes::tree_builder::{recipe_to_raw, TreeBuilder};
    use std::collections::{BTreeMap, HashMap};
    use std::path::Path;

    fn cstrings(values: &[&str]) -> (Vec<CString>, Vec<*const c_char>) {
        let owned: Vec<CString> = values.iter().map(|v| CString::new(*v).unwrap()).collect();
        let ptrs = owned.iter().map(|c| c.as_ptr()).collect();
        (owned, ptrs)
    }

    /// Reads the on-disk tag tree into (tag id, members) pairs, mirroring how a
    /// Java host would walk its in-memory tag registry.
    fn load_tag_pairs(dir: &Path, prefix: &str, out: &mut HashMap<String, Vec<String>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                load_tag_pairs(&path, &format!("{prefix}{name}/"), out);
            } else if let Some(stem) = name.strip_suffix(".json") {
                let file: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                let values = file["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other["id"].as_str().unwrap().to_string(),
                    })
                    .collect();
                out.insert(format!("minecraft:{prefix}{stem}"), values);
            }
        }
    }

    #[test]
    fn ffi_build_matches_native_build() {
        let recipes = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY")).unwrap();
        let mut tag_pairs = HashMap::new();
        load_tag_pairs(Path::new(env!("TEST_TAGS_DIRECTORY")), "", &mut tag_pairs);
        let inventory: Vec<Item> = serde_json::from_str(
            &std::fs::read_to_string(env!("TEST_FAKE_INVENTORY_FILE")).unwrap(),
        )
        .unwrap();

        let builder = unsafe { ccmk4_builder_new() };

        for (tag, members) in &tag_pairs {
            let tag_c = CString::new(tag.as_str()).unwrap();
            let member_refs: Vec<&str> = members.iter().map(String::as_str).collect();
            let (_owned, ptrs) = cstrings(&member_refs);
            assert!(unsafe {
                ccmk4_builder_add_tag(builder, tag_c.as_ptr(), ptrs.as_ptr(), ptrs.len())
            });
        }

        // Feed each recipe through the slot API the way Java would (one call per
        // occupied slot); a slot required `count` times is sent `count` times.
        for recipe in &recipes {
            let Some(raw) = recipe_to_raw(recipe) else {
                continue;
            };
            let output_c = CString::new(raw.output).unwrap();
            assert!(unsafe {
                ccmk4_recipe_begin(builder, output_c.as_ptr(), raw.yield_count as u32)
            });
            for slot in &raw.slots {
                let (_owned, ptrs) = cstrings(&slot.tokens);
                for _ in 0..slot.count {
                    assert!(unsafe { ccmk4_recipe_slot(builder, ptrs.as_ptr(), ptrs.len()) });
                }
            }
            assert!(unsafe { ccmk4_recipe_commit(builder) });
        }

        let id_refs: Vec<&str> = inventory.iter().map(|i| i.id.as_str()).collect();
        let (_owned, id_ptrs) = cstrings(&id_refs);
        let quantities: Vec<u32> = inventory.iter().map(|i| i.quantity).collect();

        // Build twice to exercise both the cold (prepare) and cached paths.
        let mut runs = Vec::new();
        for _ in 0..2 {
            let result = unsafe {
                ccmk4_build(builder, id_ptrs.as_ptr(), quantities.as_ptr(), quantities.len())
            };
            assert!(
                !result.is_null(),
                "ccmk4_build failed: {}",
                unsafe { CStr::from_ptr(ccmk4_last_error()) }.to_string_lossy()
            );
            let mut via_ffi = BTreeMap::new();
            for i in 0..unsafe { ccmk4_result_len(result) } {
                let id = unsafe { CStr::from_ptr(ccmk4_result_id(result, i)) }
                    .to_str()
                    .unwrap()
                    .to_string();
                via_ffi.insert(id, unsafe { ccmk4_result_quantity(result, i) });
            }
            unsafe { ccmk4_result_free(result) };
            runs.push(via_ffi);
        }
        assert_eq!(runs[0], runs[1]);

        let mut tags = Tags::default();
        for (tag, members) in tag_pairs {
            tags.add(tag, members);
        }
        let via_native: BTreeMap<String, u32> = TreeBuilder::new(recipes)
            .build(inventory, &tags)
            .unwrap()
            .into_iter()
            .map(|i| (i.id, i.quantity))
            .collect();

        assert!(!runs[0].is_empty());
        assert_eq!(runs[0], via_native);

        unsafe { ccmk4_builder_free(builder) };
    }

    #[test]
    fn ffi_reports_errors_without_crashing() {
        // Null builder.
        let out_c = CString::new("minecraft:stick").unwrap();
        assert!(!unsafe { ccmk4_recipe_begin(std::ptr::null_mut(), out_c.as_ptr(), 1) });
        let err = unsafe { CStr::from_ptr(ccmk4_last_error()) };
        assert!(err.to_string_lossy().contains("builder"));

        let builder = unsafe { ccmk4_builder_new() };

        // Slot/commit without begin.
        assert!(!unsafe { ccmk4_recipe_slot(builder, std::ptr::null(), 0) });
        assert!(!unsafe { ccmk4_recipe_commit(builder) });

        // Null id inside a token array.
        assert!(unsafe { ccmk4_recipe_begin(builder, out_c.as_ptr(), 1) });
        let bad_tokens = [std::ptr::null::<c_char>()];
        assert!(!unsafe { ccmk4_recipe_slot(builder, bad_tokens.as_ptr(), 1) });
        let err = unsafe { CStr::from_ptr(ccmk4_last_error()) };
        assert!(err.to_string_lossy().contains("tokens[0]"));

        // Empty build succeeds and returns an empty result.
        let result = unsafe { ccmk4_build(builder, std::ptr::null(), std::ptr::null(), 0) };
        assert!(!result.is_null());
        assert_eq!(unsafe { ccmk4_result_len(result) }, 0);
        assert!(unsafe { ccmk4_result_id(result, 0) }.is_null());
        assert_eq!(unsafe { ccmk4_result_quantity(result, 0) }, 0);
        unsafe { ccmk4_result_free(result) };

        // Frees of null are safe no-ops.
        unsafe { ccmk4_result_free(std::ptr::null_mut()) };
        unsafe { ccmk4_builder_free(std::ptr::null_mut()) };

        unsafe { ccmk4_builder_free(builder) };
    }
}
