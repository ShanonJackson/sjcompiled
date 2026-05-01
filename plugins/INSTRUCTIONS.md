We are building a SWC equivalent to packages/babel-plugin inside crtes/babel-plugin (Rust equivilent)
ALl files/folders of original will be implemented, but translated to Rust. Outputs for all Inputs will be identical in all cases without fail.



These are the hard constraints:
1. Ignore "resolver" in plugin configuration we can't make this 1:1
2. When you need webpack enhanced resolve use the 'crate' oxc_resolver https://crates.io/crates/oxc_resolver it works the same way.
3. When you need packages/css/src/transform.ts; Temporarily create a NAPI binding that will allow you to call into that JS code, In parallel with you an agent is working to rebuild this in Rust; But for now you'll need to call into the .JS one. The rust one will be identical in every way, every output will return identical input.
4. File/Folder structure is IDENTICAL between packages/babel-plugin and crtes/babel-plugin and very file is 1:1 to it's JS counterpart. If you ever feel like you need to deviate DON"T; Ask EXPLICIT Permission
5. If we don't have an 'equivalent' of something in babel (say generate) create a crtes/<your plugin>/src/compat/*.rs file with an implementation identical to the counterpart we're missing. The implementation should not be 'half baked' as this plugin has 10_00 file usages in real code, if we 'miss' something it will be discovered when we integrate it into the project.
6. Bugs in the existing packages/babel-plugin SHOULD exist in the new SWC variant, because our goal is to build a 'drop-in' replacement, existing bugs/behaviours/quirks should stay even if they're 'incorrect'; Otherwise we risk changing css/css output in consuming applications.
7. Has to  BUILD to a WASI plugin compatible with "@swc/core@1.15.8" 

It's critical that all files/folders of the original plugin have 1:1 counterparts in the new; Matching identically just translated to JS.
If you cannot replicate something 1:1 you need to stop your work immediately and raise the issue with me and i'll make a decision on what to do.


