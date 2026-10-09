# kasane-project

Project persistence, asset management, and import and publication of model and animation resources.

The JSON writer emits directory-project format v7, v8 when an Offscreen is hidden,
or v9 when Deformer controls are hidden, and the reader accepts v1–v9.
The optional `document.deformer_display.hidden` list stores validated Deformer
IDs whose editor controls are hidden. This metadata does not affect runtime
evaluation and does not inherit to child objects. Older files default to showing
controls. The field requires v9 so older readers cannot silently lose the state.
Offscreen `enabled` defaults to true in older files;
false is persisted in v8 so older readers reject the file instead of silently
showing hidden composites. Visibility does not change the Offscreen's keyforms
or its child objects' own enabled flags.
The optional `document.object_locks` field stores validated explicit editor locks;
it is omitted when empty and is rejected on older format versions. Older projects
load unlocked. Readers supporting at most v6 reject v7 rather than losing locks
when resaving. The experimental CBOR codec shares these wire rules.
