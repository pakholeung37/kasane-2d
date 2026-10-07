# kasane-project

Project persistence, asset management, and import and publication of model and animation resources.

The JSON writer emits directory-project format v7 and the reader accepts v1–v7.
The optional `document.object_locks` field stores validated explicit editor locks;
it is omitted when empty and is rejected on older format versions. Older projects
load unlocked. Readers supporting at most v6 reject v7 rather than losing locks
when resaving. The experimental CBOR codec shares these wire rules.
