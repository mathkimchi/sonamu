# Devlog

Things I want to log for my own remembering purposes.

## Initial Brainstorm

Inspirations (Things too look at for reference):
- Emacs and Vim for flexibility with subapps and extensions
- Tree-based organization
  - Sidebery Extension
    - Tree based browser tab organization, but slightly different from how I want it.
    - Nodes are tabs, but I want nodes to be more flexible (eg if on a page with sections, those sections would show up in the tree heirarchy) and nodes could have different relationships (not all nodes has its own window or ...)
  - The structure I want resembles processes more than the tab managers, where children nodes might be part of the parent processes
- Display standards
  - Direct pixel buffers
    - This is how apps tell OS's how to display
    - [VGA Terminals](https://en.wikipedia.org/wiki/VGA_text_mode) work similarly, with 2 bytes per character, one byte for attributes (blink, 8 bg, 16 fg) and another byte for character (i think ascii)
    - Definitely do not want this
  - Html
    - Though I usually despise html, I am leaning towards a protocol closest to html right now.
    - The idea of window elements seems like it fits what I need the most.
    - I could have a finite list of allowed window elements
      - Most (if not all) would be rectangular
      - Eg: Plain text, div, button, ...
      - Though practical, I feel a philosophical distaste for this
  - Markdown
    - I like the minimalism of md for documents, but I need much more features for apps
- Rust Cargo and Nix for package management

Use Cases:
- Coding
  - Subapps:
    - Editor
    - File manager
    - Terminal
    - Browser
    - Task organizer
    - Brainstormer
    - Live viewer (for markup languages)
- Note Taking
  - Need to quickly write and organize and read notes
  - Idk, look into org mode and org roam
- General Organization
  - Calendar 
  - Tasks
  - Email
  - Time
- Music making
  - (feels like a whole can of worms I might not want to deal with)

TO REVIEW:
- Tagging
  - Archive
- Environments
  - Divide by project?
- How to deal with multiple screens?
- flexibility+accessibility
  - all core features and most features should be possible with:
    - only keyboard
    - terminal only ui
    - any os
    - audio only (idk, this is a stretch)
- Sync
  - Majority of syncing can be done through git and gh
- How to cope with HTML?
  - Even if this project gets a lot of users, and I create a display standard that can be a HTML replacement for web, I will need to access webpages that use HTML and will not directly support my heirarchy system.
  - I could just ignore that
  - I could give the user an easy way to manually split the html page to sections
    - With something like inspect element
    - Bloat might interfere with this
  - I could give custom extensions for famous webpages based off their api, or converting their html into something else
  - I could just directly support html
    - Probably will break on most complex websites
- Scripting language
  - Rust is a compiled language, how can I support runtime scripts and extensions
- Figure out how the following categories are different and their relations to each other
  - Global Data
  - Subapp Global Data
  - Subapp Instance Data
  - Frontend Data
  - Node
  - UI Element
  - App Instance
  - Environment
  - Window
  - Subapp
  - Tag
  - Project
- Reference Nodes
  - Analogous to symlinks
  - If multiple projects require the calendar, it would be beneficial to have multiple references to one instance calander app.
- Automatic Tree Organizer
  - Minimizes the total distance * frequency of switch of all pairs of two nodes
  - I probably wouldn't use this, but it could be fun
  - Speaking of this, what about cool telemetry (stored locally, opt in) for fun like those graphs in obsidian or smth
- Element
  - Elements are displayables that subapps can use to avoid rewriting code.
  - For example, the textbox element and tree view element
  - Having this also makes it easier to standardize subapps

Maybe the thing that I am looking for is a project management app.
One of the things I want to change is that I don't like the way apps are organized.
It feels ineffecient for some reason. The current organization is divided by:
desktops, then windows (where a window is a single app), then the app's own organization system.
If I am coding, I usually have multiple apps, at least an editor and a browser for a single project.
That means I have to constantly switch between multiple UIs and multiple windows.
When I close and open the project, I then have to reopen all the related things in two different systems.
Is this a really specific minor inconvenience? Yes, and I want to fix that.
I want singularity to set a system where all these different subapps can be organized within the same system.
A good rule of thumb for effecient organization is that things that I switch between often should be organized "close" to each other.

Ultimately apps are just friendly ways to display and modify data.
Its pretty obvious, but I just wanted to say it.
Every project will have corresponding data (not necessarily centralized)

Short-term Development Plan:
- I am just going to start by focusing on the bare minimum coding environment for a single project at a time
- Stick with TUI for now
  - Give subapps buffers from the ratatui library
    - Each cell or character has the following attributes:
      - symbol (the actual character), fg color, bg color, underline color, modifiers (which is a whole bunch of new stuff, go to ratatui::style::Modifier for options, use `|` to chain multiple), skip (idk what skip is)
  - No cursor management, if needed, modify attributes to manually emulate cursors
  - Need for proper gui feels more and more usefull
- Nodes in the tree are either empty (organizational groupers) or an individual window
  - I'm going to have the organization similar to the tree tab extensions
- Subapps to do:
  - [x] Editor
  - [x] File Manager
  - [ ] Terminal
    - skip for now
  - [ ] Task organizer
- Task system:
  - Just consider 1 file for now, no global, no references between projects, don't even care about the project directory as a whole.
  - store in a json file, edit and view with task organizer
  - [ ] Task
    - [ ] Head
    - [ ] Body
    - [ ] Check
  - [ ] Subtasks
- ui elements
  - textbox
  - tree view
  - Don't need a trait for elements (at least not right now)

---

2024/8/23

Okay, I am at the very very early stage where I can barely say that the individual subapps (text editor, file manager, and task organizer) have enough features such that they can symbolize what they are supposed to be.
I have so many ways of improving them right now, but I know that I will always have ways to improve the details.
The thing I must do now is to work towards my vision of the bigger picture, and first determining what that even is.

Over the past few weeks, as I implemented these subapps, I had time to specify my abstract idea of an "all in one app."
The goal for singularity is to increase the user's productivity.
I've thought a lot about the principle of making things customizable if they can not be perfect.
The main feature that this aligns with this idea is allowing anyone to write subapps.
I want singularity to provide tools for subapps so that all subapps that use those tools to be standardized to some extent.
These tools will usually come in the form of abstraction, like abstracting the UI and organization.
The problem is that I can't consider every single use case, so I am going to start with mine.
My ideal version of singularity would allow me to use it for every single productive thing I can do on my computer.
These are (with overlap):
- coding projects
- note taking
- writing essays
- brainstorming ideas
- making music
- email management
- memory management
- event reminders
- task list
- homework
- small scale time management
- logging
- journaling
- organizing
- web browsing
- reading
- watching videos
- writing proofs
- searching for information (both online and on my system)

The organization system goes like this:
- every project has a specific corresponding project folder containing:
  - core file
    - its project id
    - subprojects
      - the subproject name/id
      - where to find it
    - subapps used
      - subapp name/id
      - where to find it
      - subapp settings
        - standard subapp settings
          - settings that are used by the manager rather than the subapp
          - these settings exist for all subapps
          - ex: their file permissions
        - subapp specific settings
      - NOTE: subapps and their settings are extended to subprojects unless specified otherwise
  - owned files called property for each subapp that requests it
- every subapp has:
  - subapp id
  - the runnable
    - haven't decided on what this is yet
  - dependencies
for a given user, their projects might look like this:
- project: root project (more like user configs)
  - meta:
    - standard: basic color pallete
    - code editor: format on save
  - diary subapp data:
    - I worked on cool coding project (link to cool coding project -> devlog subapp -> bugfix) today
  - children:
    - project: cool coding project
      - devlog subapp data:
        - bugfix
    - project: another coding project
      - meta: code editor: don't format on save
    - project: physics
      - children:
        - project: momentum notes
          - meta:
            - standard: tags: archived
        - project: collision notes
          - notes subapp data:
            - collision preserves momentum (link to momentum notes).

I guess a project file organization standard could be a whole different thing.
But I want to be as unintrusive as possible so people who don't use singularity won't be negatively affected because a project file organization standard does comply with this standard, and vice versa.
Having a single folder with all the singularity stuff would be the best way to do this I think, like a shell.nix file or a .vscode folder.
If people don't want the singularity stuff to bloat their project repository, they can .gitignore it.

Speaking of seperating the roles of singularity, these are the components that are needed to make it work:
- subapps
  - each subapp only talks directly to the manager
  - should not even directly access the filesystem (though I might not be able to force this)
  - but, the manager can then talk to the UI or another subapp or the file system on behalf of the subapp
- UI
  - displays from `SAVDR` (standard abstract visual display representation which is like HTML, should take care of **most** usecases)
  - turn user input into `user input events` and passes it to manager
- manager
  - this is what the core of singularity is; what connects all the components
  - takes care of subapps' permission for files
  - provides the proxy between subapp and ui
- file system
  - where long term data is stored

I am not sure how I will implement subapps to be modified at runtime.
It seems that I am looking for a method of [IPC](https://en.wikipedia.org/wiki/Inter-process_communication) (inter-process comunication)
Here are some possibilities roughly ordered from ideal to horrible:
- dynamic library
- cli
- Manager-subapp communication via sockets
  - unix domain socket
  - still have to figure out initialization
- [Rhai](https://rhai.rs/book/start/index.html)
- shared memory
- message queue
  - i think this is similar to what `ManagerProxy` is doing
- wasm
- Custom language
  - please don't do this
Research:
- https://3tilley.github.io/posts/simple-ipc-ping-pong/
  - goes over many ways of ipc between rust
  - uses shared_memory crate for shared memory
  - uses Commands to spawn rust
- Search `crates.io` for ipc
  - d-bus is a linux tool for ipc
    - is pretty widely used
    - i think it uses servers
    - does not use sockets or shared memory
    - there seem to be people who hate it, but posts about why it is bad are also often met with ppl defending it
    - many crates for it, like `dbus` and `zbus`, both very famous
    - is probably not fast
  - `parity-tokio-ipc`
    - uses unix stream for unix and named pipe for windows so it is flexible
  - `interprocess`
    - idk, not that popular but has a lot of features
    - uses sockets and unix domain socket
- Search crates.io for shared memory
  - `rustix`
    - very popular, has many features, including shm. But, it doesn't focus much on shm and in fact seems to lack documentation.
    - Unless I plan on using its other features (which I might), a shm focused crate would be better
    - Has bad documentation
  - `shared_memory`
    - i mean it is called shared memory
    - needs `raw_sync` crate
    - hasn't been updated in a year
- https://users.rust-lang.org/t/shared-memory-for-interprocess-communication/92408/8
  - Use `pthread_mutex` from `libc` crate
- https://www.youtube.com/watch?v=RtVzlk4om6U
  - uses just the std library, std::process
  - Command to spawn
  - stdin, stdout, stderr pipes for communication
- manual shared memory implementation with no crates
  - it might not be too hard:
  - https://stackoverflow.com/questions/66621363/can-you-cast-a-memory-address-as-a-usize-into-a-reference-with-a-lifetime
  - it will definitely be unsafe but i think i could make it work
- How x window system does client-server communication:
  - I realized that singularity is very similar to a window manager
  - https://en.wikipedia.org/wiki/X_Window_System_core_protocol
  - Wayland (which i use personally) has a [similar article](https://en.wikipedia.org/wiki/Wayland_(protocol)#Wayland_core_interfaces) but it focuses on different things
  - overview section:
    - packets sent via network channel
    - Four types of packets: request (client requests server to do st or requests attributes like window size), reply (server respond to request), event (server informs client about relevant event), error (server tell client that request is invalid)
  - Graphic contexts and fonts:
    - `The client can request a number of graphic operations, such as clearing an area, copying an area into another, drawing points, lines, rectangles, and text. Beside clearing, all operations are possible on all drawables, both windows and pixmaps.`
    - this is pretty wild, i think this means that x window system clients do not get to directly manipulate a buffer, instead needing to request modifications
    - I feel like this would be super super slow, if videos and games are forced to go through this as well. I assume there are other ways to do it as well.
    - wait, i think that is what pixmaps are. I literally read the pixmaps section too, but i misinterpreted it
- Takeaways from Command and pipes attempt:
  - Feels unpredictable
For now, I am going to use Command to spawn and pipes to communicate.
If I need speed, I will look further into shared memory, but I just want it to work right now.
I assume pipes only lets strings or bytes through so I will use serde to send custom types.
In terms of organization, I am going to try turning the main logic stuff into a library and each subapp into their own package with binaries.
I think it is possible to let the manager and main logic have a library and a binary, but if not I will make manager a binary and the main logic a library.

This didn't work, new idea is to try using unix sockets with vanilla rust, but I am open to using rustix. Still spawning with Command.
I will start every message with a length of the actual message.

My ideas are still pretty broad, but I think I can make new progress based on what I wrote so far.
The next step is:
- [ ] implement project organization system
  - [x] (manually) make a test directory
  - [x] make project class and parser
  - [x] make project manager class
    - [x] each instance of project manager corresponds to exactly one project
  - [ ] get task organizer to work with project manager
    - [ ] add a way for subapps to talk to project manager (either replace `ManagerProxy` or make it better)
      - [x] split the subapps from singularity
  - [ ] add project heirarchy
  - [ ] add linking/referencing to task organizer

## Dynamic Subapps Research

2024-09-06

I realized that my research on rust IPC was too specific, what I really care about is just runtime plugins in rust.
I think I saw [this reddit post](https://www.reddit.com/r/rust/comments/144zmwk/how_can_i_add_dynamic_loading_to_do_plugins_for/) already, but I looked through it again and found a tutorial on the very thing I am looking for.

Notes on [the tutorial on rust plugins](https://nullderef.com/series/rust-plugins/):
- Goes over a bunch of ways to do rust plugins
- Dynamic libraries
  - libloading is the main crate, even bevy uses it
- wasm is the other interesting option

I will try libloading, and this time, I will use branches so I don't mess up the main branch.

Speaking of branches, my current plan for branch organization is to follow [this convention](https://medium.com/@abhay.pixolo/naming-conventions-for-git-branches-a-cheatsheet-8549feca2534):
- `main` branch
  - should be thoroughly tested and stable (isn't right now), if someone wanted to use the most up-to-date version of singularity, they would use this
  - ie, this is the most recent stable release
- `dev` branch
  - tested, but not ready for users
- `feature/abc-xyz-0`
  - for new features

There are also bugfix, hotfix, and documentation branches, but I won't use them because it is just me.
In other words, all new branches will be in the form `feature/some-description`

I trust bevy, and its [code for dynamic loading](https://github.com/bevyengine/bevy/blob/v0.5.0/crates/bevy_dynamic_plugin/src/loader.rs) is extremely simple (just 25 lines), so I will try following it.

---

2024-09-07

Wait, NOOOO, bevy's dynamic plugin loading is deprecated and will be removed in the next version (0.15)!

You know what, I have had just about enough of the errors and dead-ends with dynamic plugins,
and in trying to do dynamic plugins, I learned that as long as I have a trait for plugins (in my case subapps),
then adding dynamic loaders for those won't require modifying the existing code much.

## Tabs

2024-09-07

I am going to use the term `tab` for a plugin/subapp/extension/feature that is in charge of exactly one thing being displayed.
For now, I will rename all subapp to tab, because subapp is hard to define.
Tabs will have a corresponding buffer, and tabs can run on their own threads.

I want to ensure that the manager itself never has to wait when the tab is doing something,
and I will do that by making a tab handler that will talk to the tab, which will run on a different thread.
So, the manager will have to just wait for the tab handler, which will be written in either the core or the manager crate.
But, I don't particularly care if the tabs are forced to wait for the manager.

I don't really want to annoy myself with the specifics of IPC again, so I will use a simple and plain message queue.

The mutex might require waiting for it, I am actually not sure.

2024-09-08

I actually love rust, it turns out there is a [chapter in the rust book](https://doc.rust-lang.org/book/ch16-00-concurrency.html) about what I want to do.
Here are my notes:
- Send messages between threads
  - mpsc
    - multiple producer, single consumer
  - multiple tx (transmitter)
  - single rx (reciever)
- Shared state concurrency
  - Share data, with `Arc<Mutex<T>>`

There is also the `async` keyword, which I forgot about.
This also seems usefull.
There is a whole [book](https://rust-lang.github.io/async-book/) on it.

2024-09-10

To recap my problem, what I want is to send:

- Events (Enum) from server to client
- Requests (Enum) from client to server
- Query (Enum) from client to server
  - Response from server to client, want this to correspond to each query
  - Ideally, there would be a way to enforce the type of corresponding query-responses. Like, if there were two queries: GetFloat and GetInteger

I learned that:
```rust
enum Enum {
    A = 0,
    B = 1,
}
```
is a thing.
It only works with isize values, and I think it just has to do with how rust stores enums.
Associated consts are also kind of similar.
But, these aren't exactly what I want so for now, I am going to keep it simple and not enforce type correspondance.
If I am to do this later, I might have an enum for Query-Response type, Query, and Response.
Or, I could have a Query trait with the `T=` thing.

The most ooga booga way of doing this would be to have a mpsc pair for each of the packet types (Event, Request, Query, Response).
I am going to do the ooga booga way, because the logic is going to be abstracted anyways, so I can easily change it later.

---

2024/9/13

I just realized that I might need to have a seperate function and channel for each possible query if I want it to be all type safe.
You know what, I am just going to try doing whatever works, and try not to think about it too hard.

Later on, I might make a macro to automatically make a function for each query-request.

---

2024/9/14

Today, I will work on acually letting tabs display stuff.
Here are the ways I thought of:
- Plain mutex of display buffer without extra logic
  - Works, but if a tab writes for a really long time, then the manager will be stuck. (I am assuming)
- Mutex of display buffer and the manager only updates if the mutex is currently not locked
  - I assume this is possible
  - Is better, but is kind of bad if the tab locks for a long time and unlocks for a short time repeatedly
- Enforce some form of double buffer
  - Only problem I can think of right now is changing the size of the double buffer on manager side
  - I did this, and it works for now.

I think I am done with the tab backend.
The next big category of things to work on is figuring out how projects will work.

<!-- ## Project Organization

2024/9/14

I've already decided on organizing by projects, but within that, I am not sure how to further organize.

So, I am going to start by exploring how other apps do this.

- Nixos
- Obsidian Vaults
  - I've used obsidian for school notes, I liked a lot of things about it and it is a pretty good productivity app
  - Markdown files can link to each other, though I've never done it
  - Tbh, I don't know how Obsidian actually does this stuff
- `.vscode`
  - When you change a setting for a specific project in VSCode, it stores the new settings in `.vscode/settings.json` or something like that
  - This is simple and gets the job done
- Org mode
  - I tried to use Emacs before but I didn't like it (which is partially why I am making this). From what I've heard, one of the key selling points of Emacs is something called Org mode. -->

## Improving UI

2024/9/14

Never mind, I think I should improve the UI first.
The reason why I want to do this is because when I thought about actually using singularity, I realized that I would need to improve the UI (mostly display rather than input) but I could still use singularity if it didn't have project organization.

So, I am going to either look into actual GUIs in rust, or just make a really good TUI.
(Later, I want singularity to have an agnostic UI though.)

### Researching Rust GUIs

2024/9/14

I am not sure if I should do a full GUI or improve the current TUI, but I will research my options.

I want something that easily allows me to designate certain rectangles to different tabs.
Currently, for the TUI, I give a buffer to each tab.

- `wgpu`
  - very fast, might be overkill
    - Looked at code for displaying triangle, definitely overkill
  - bevy uses this
- `gtk` and `glib`
  - made by gnome
  - rust is on their [home page](https://www.gtk.org/), so it probably has relatively good support
  - I might need to install gtk if i want to develop or run this
- `egui`
  - Supposedly the easiest rust gui
- `iced`
  - Compared a lot to egui
- `glutin`
  - BIG ADVANTAGE: can embed Servo, a rust browser engine
- `wayland-client` or `smithay`
  - both surprisingly popular (~10mil downloads)
  - I use wayland and no one else will use singularity anytime soon, but I still want it to be as cross-platform as possible
  - pretty low level

I'm just going to try doing egui and see what happens.

Egui allows custom widgets, so I might be able to use that to split areas for tabs.

---

2024/9/15

Egui isn't really hard, but I feel like it is too much boilerplate and it doesn't have a lot of examples.

I came across `winit` which seems to be a bare-bones thing, and that might actually be better for this purpose so I am going to try that.

Running the most basic winit code: `EventLoop::new().unwrap()` returns an error `error: WaylandError(Connection(NoWaylandLib))`, probably because of some Nixos thing.
An `egui` and Nixos user had [a similar issue](https://github.com/emilk/egui/discussions/1587) and they fixed it with a flake.

Tbh, I don't really feel like actually learning how to use nix flakes right now, so I am going to try changing my entire configuration to fix this.

---

2024/9/16

I couldn't change my entire configuration to fix this, so I just added the flake from github and then did `nix develop`.
I promise I will learn nix flakes, and when I do, I will make my own flake.nix for singularity.

But, the good news is that winit works now on my machine, my code right now doesn't, but I know winit works because when I do `nix develop` on this directory and then go to the directory storing winit then run `cargo run --example window`, then it shows a blank window.

I guess nothing is showing on my code because of [this](https://github.com/rust-windowing/winit/issues/776) bug involving wayland.
Apparently the change to fix this was closed.
The bug is that wayland doesn't show windows until something is drawn on it.

I was trying to draw something really simple just so I could see the window, but as it turns out, that is really hard to do with juset winit.
I guess I will use wgpu then to draw stuff.

Honestly, I might need to make another crate just to abstract all the UI nonsense.
I am hoping this will make development easier, and that it will also help when I make singularity agnostic down the line.
So, I will do that after committing to save progress.

Apparently Cosmic DE uses iced, so I am going to see what iced is like.

### UI Abstraction

2024/9/18

I want there to be a display, which is like the os window.
With a display, I want to be able to be able to split rectangular sections out of it and give those sections to each tab.

Because of rust's mutability safety restrictions, I am considering a system where modifying the a rectangular region doesn't update the display until the display's `fn update(&mut self, region: Region)` is called.
I am not entirely sure what methods are fast enough, so if this is noticably slow, I might have to rewrite everything.

Another way might be to utilize the graphics package (iced)'s existing systems like widgets.

2024/9/20

I am considering an element system, something similar to HTML.
Instead of giving subapps buffers and giving them elements as a tool to modify the buffer, I can force them to use elements by making them return elements or modify elements.

2024/9/23

...it is actually not as simple as I thought.
The three ways I can think of doing elements is:
- Element is data, and shared with mutex. To update, just change data. Could possibly also notify element updates
  - Feels like it should be faster than having immutable data, but ultimately, I am not sure if this much faster. Suppose there is a large nested element and a small part of it is changed. Updating this would be no different than 
- Element is immutable and the tabs need to send a new element every time they want to update
  - Don't like this one
- Element is trait, and shared with box (maybe mutex is also needed)
  - If owned by main app
    - Main app can call an update function of element, and when this is called, it somehow gets data from the tab (reciever, or the main app passes data from tab to the element when calling the function)
    - I assume this is how iced does it
  - If mutex
    - Send data by modifying element

After considering all my options, I am considering either the element is data or element is trait and owned by main app.
I am going to try the data mutex one, and if it doesn't work I will try the trait one.

2024/9/24

Running the UI pauses the thread until the UI is exited, and I can't run winit stuff in a seperate thread either because Mac forces all UI calls to be in the main thread.

The loser way would be to run all the other stuff in a seperate thread, but that is so lame that I would rather just switch back to egui.
Apparently winit has a way of allowing non-main thread execution, but the problem with that is that I don't care.

I might actually need to switch to egui, so I am going to commit before I go any further.

Okay, egui also uses winit, BUT, apparently egui [allows me to easily allow non-main thread running](https://github.com/emilk/egui/discussions/1489).
But, I need to use winit, and eframe 0.28 and winit 0.30 are incompatible or something.

2024/9/25

I added active updates, but currently you need to give it an event to make it update.
I can jankily do continuous updates by just requesting updates on every update.

The basic foundations for all the features (ui elements, ui events) have been implemented, so I am going to implement the following, and as I do that, I can add necessary features:

- [x] text editor
  - [x] more variety of keyboard input
    - [x] ~~modifiers~~
    - [x] ~~more characters~~
    - I just exposed egui's stuff to anything using singularity ui
  - [x] char grid element
    - more or less a tui display, with monospace font and possibly basic visual features like colors
- [x] tab traversal
- [ ] tab selection with mouse
  - for the prototype, just make it so that mouse is used only to select a tab
  - SKIP for now
- [ ] window management with tabs
  - [x] tab's tree hierarchy should not define position
  - [x] tab tree hierarchy should not define display order
  - [x] make tabs able to overlap
  - [ ] able to move tabs around with keyboard
    - [ ] maximize focused tab with `Ctrl+Shift+Up`
    - [x] minimize focused tab with `Ctrl+Shift+Down` (and change focus?)
    - [ ] move focused tab to the side with `Ctrl+Shift+<Right/Left>`
  - [ ] close tabs with `Ctrl+Shift+W`

---

2024/9/27

I was going to implement tab traversal, but egui's philosophy is getting in my way.
I want to know how Zed does gui, so I am reading a [blog from Zed](https://zed.dev/blog/videogame).
- the blog actually mainly goes over gpu programming, and singularity is not ready for that yet.

I guess I will continue using egui, but I will just try to have the backend not matter as much as possible.

---

2024/9/28

Since my philosophies on gui seems to not be shared by other gui frameworks, I will try to go lower level until I can just implement it myself.

Right now, I created a new directory called testing which is completely unrelated to everything else, I am just using it to test how it would be to use wayland-client. If this doesn't work, I might need to do winit+some gpu programming.

Okay, so with just wayland-client (no smithay client toolkit), it definitely feels possible but I think if I tried to rawdog it, I would be wasting my time.
I was looking at [this example](https://github.com/Smithay/wayland-rs/blob/master/wayland-client/examples/simple_window.rs) for wayland client.
Anyways, I am now going to try out smithay client toolkit, and luckily for me, there is an actual [tutorial](https://smithay.github.io/book/client/sctk/environment.html) on this, which I have learned not to expect from most rust crates.

I had to add a few dependencies to even get it to compile, but that is not a big deal.
However, the code from the tutorial is not actually working, and links to the actual documentation in the tutorial are also not working, so I assume that the tutorial is outdated.
Smithay's `Environment`s seems to have been removed since version 0.17, and 0.19 is the current newest.
This is not very reassuring, especially the fact a feature so widely used that it was in the tutorial could be removed in an update.

I feel like this is going to be a whole can of worms, so I don't think I should implement wayland and smithay at this stage.

I will try looking into Zed's gpui, but if I start feeling like it isn't much better than egui, or that it is just too complicated, I am going to commit then undo asap.

I tried it, but there was a very long error message ending with `cannot find -lxcb: No such file or directory collect2: error: ld returned 1 exit status`
and it probably has something to do with Nixos, but I will keep true to my word and abort this tangent before it consumes any more of my time.

---

2024/9/29

Here is a cool snippet: `find . -name "*.rs" -type f -not -path "./target/*" | xargs wc -l`:
- `find .`: list all items in the directory with the following conditions:
  - `-name "*.rs"`: ends with `.rs`
  - `-type f`: is a regular file
  - `-not -path "./target/*"`: is not under the `target` directory
- `xargs`: changes one type of input to another (not rly sure about this one, it just works) in this case, stdin to argument
- `wc`: displays line count, word count, and char count
  - `-l` displays line count only
- In short, this displays how many lines I've written in this directory.

I wrote 3057 lines, 9083 words, and 101606 characters of rust.
The top three biggest files are:

| file            | lc  | wc   | cc    |
| --------------- | --- | ---- | ----- |
| task organizer  | 318 | 1083 | 11996 |
| text box        | 317 | 1006 | 10381 |
| project manager | 313 | 773  | 10997 |

and the top two of these are currently not being used, so my line count is pretty inflated.

### Decoupling Tabs

2024/9/29

In order to seperate the tab tree hierarchy from the view, I will need to redo how I store tabs.

The current way is to only use rooted trees.
This worked when the tree hierarchy determined how things were viewed, but now I also want to set an order to rendering.

My ideas are:
- Store tabs + z-order in tree
  - Sort by this before every render
- Store tabs in tree+store a seperate vector of paths to tabs in order.
- Store tabs in vec, store seperate vector of indices pointing to the tabs for render order, also store the tree hierarchy with tree of indices
  - I like the idea of this, but there is just one change to make this better
  - The problem is that any modification to the order (caused by closing a tab) would mess up everything
- Each instance of a tab has an immutable uid, store tabs in a vec/btreemap, store hierarchy and render order via the id

### Exploring Alternative Frameworks

Egui is not letting me manually set the sizes of widgets.

I am going to try wayland client once more.

In the sctk (smithay client toolkit), there is an example called relative pointer.
I couldn't get the example itself to work, but it uses a crate called raqote and font kit.
I previously could not write actual text, but I guess these crates can render text for me.

For their window example, they use minifb, and the minifb looks very simple and good, but it isn't super widely used, so I'll only look into it if wayland doesn't work.

So right now, for nested items like Containers and Borders, I create an entirely new buffer to store data for it, then I try to copy it onto the parent buffer.
As you can imagine, this is not great for speed.
Actually, nvm the real problem seems to lie with the text, so I was going to optimize the buffer thing but I will fix the text first.

Also, every once in a while, the lengths of the canvas and draw target (which I am using as the buffer) don't match, even though they should be the same thing.
- I think this happens when I resize, but it is non-deterministic.
- Okay, I think it happens when a double buffer is created.
  - All crashes happen on double buffer creation, but double buffers can be created without crashing
- Hmm... a consistent way to crash is by resizing it to the left or right extremes.
  - When breaking this way, it breaks on the time it creates a double buffer
  - Can happen on other resizes though
- The canvas is slightly longer than the draw target. The draw target matches `4*width*height` which is same even when canvas is created. The canvas is greater by a multiple of 4 (from around +4 to +36)
- I fixed it really jankily by not drawing when the sizes are wrong

TODO: keyboard handling abstractions

---

2024/10/01

I am going to try to optimize the displays.
- Currently, for nested elements like containers and borders, I create a new draw target. I did this for "safety," aka ensuring that elements wouldn't draw outside of its given areas.
  - However, I don't need to do that with the current system because I can just make sure the elements follow the rules when they are drawing.
  - I did this, and it is still noticably slow...

To quantify performance, I will log times.
I am running this on my laptop at full screen with quite a few background processes, notably VS Code and Firefox (with most tabs unloaded).

Right now, the general output is roughly:

```log
Starting drawing. 3.991012ms elapsed since last finished drawing.
Starting rendering. 4.025498ms elapsed since last finished drawing.
Started drawing elements. 4.202639ms elapsed since last finished drawing.
Finished drawing elements, starting copy. 398.530553ms elapsed since last finished drawing.
Finished rendering. 399.172312ms elapsed since last finished drawing.
Finished drawing. 399.266227ms elapsed since last finished drawing.
```

This isn't the average or anything, but it gives a good sense of the magnitudes and we can see the rendering taking almost all the time.
I thought copying from dt would take a lot of time, but I was wrong.

My next optimization will be to render fonts once and reuse it instead of rendering it each frame.
It turns out that Font doesn't implement Sync, so I need to pass a fonts parameter whenever I call draw.
Okay, this didn't help much, the result is:

```log
Starting drawing. 4.202717ms elapsed since last finished drawing.
Starting rendering. 4.243526ms elapsed since last finished drawing.
Started drawing elements. 4.736223ms elapsed since last finished drawing.
Finished drawing elements, starting copy. 392.39039ms elapsed since last finished drawing.
Finished rendering. 393.597623ms elapsed since last finished drawing.
Finished drawing. 393.689893ms elapsed since last finished drawing.
```

I might revert this unless I absolutely need to.

```log
Starting drawing. 5.521291ms elapsed since last finished drawing.
Starting rendering. 5.560596ms elapsed since last finished drawing.
Trying to get root element. 6.125869ms elapsed since last finished drawing.
Got root element, starting drawing elements. 6.128967ms elapsed since last finished drawing.
Finished drawing elements, starting copy. 418.385396ms elapsed since last finished drawing.
Finished rendering. 419.475338ms elapsed since last finished drawing.
Finished drawing. 419.533916ms elapsed since last finished drawing.
```

I wondered if locking the mutex might've been holding us back, but it seems to take the least time.
(I already thought of ways to make this efficient. good job, past me)

I made a tool to log, and when I logged, I got:

```log
Starting 'fill rect'...
Finished 'fill rect' in 252.765µs.
Starting 'draw character'...
Finished 'draw character' in 9.04µs.
Starting 'fill rect'...
Finished 'fill rect' in 252.673µs.
Starting 'draw character'...
Finished 'draw character' in 6.743µs.
Finished 'draw char grid' in 367.730814ms.
```

So, it seems that drawing character grids is taking a significant bit of my time.
Specifically, the 'fill rect' is taking enough time that it is noticable when there are potentially thousands of characters being rendered.
I asked google, and 2µs * 1000 = 0.2s = 200ms, so it seems I caught the culprit.

I don't know what is wrong with raqote, but I might need to get lower level than raqote by doing some gpu stuff myself.
While I am on this topic, I want to log an idea I had:
- Chunking
  - Squares (or maybe 1x2 rectangles like a terminal character) of constant size, probably like 8x8 or 16x16
  - Each chunk has an owner
  - More or less a pixel buffer, but with extra steps
  - Benefits:
    - Possibly faster than pixel buffer
    - Feels like an upgraded version of the terminal
  - Problems:
    - Very rigid, can't resize smoothly
    - Possibly slower than pixel buffer because wayland doesn't store data this way
Anyways, here is a roadmap for me:
- [ ] Modify an array of u8 / u32s with gpu
- [ ] Pass data to gpu
- [ ] Render text
- [ ] Implement each element individually

Uhm, this is really awkward, but it suddenly works now.
I wrote some optimizations before writing the previous paragraph, and didn't bother to test it out because I thought it wouldn't work.
I was secretly kind of looking forward to learning gpu, but I guess I can't be complaining.

2024/10/02 future me here, I should have looked further into `tiny skia` instead of trying gpu.
The main page of tiny skia says raqote is very slow, and the benchmarks support that to a high degree.
One problem is that it doesn't have text though, which is a pretty big problem.
Still, my point is that raqote is very slow.

### GPU

2024/10/01

You know what, GPU time.
- [x] Modify a mut slice of u8 / u32s with gpu
- [x] Pass data to gpu
- [ ] Render text
- [ ] Implement each element individually

I think I'll go with vulkan instead of opengl, since it seems Zed and Cosmic DE both use vulkan (can you tell yet that I am unable to form my own opinions?).
The commonly used vulkan crates seem to be:
- ash
- wgpu (not vulkan specific, but supports vulkan)
- vulkano
  - people talk about it a lot, but much more people use ash than vulkano

Actually, smithay has a wayland-egl crate, which I guess will work nicely with everything else from smithay, and it supports both open gl and vulkan apparently.
However, it seems very sparse in documentation.
Also, smithay's own [gpu example](https://github.com/Smithay/client-toolkit/blob/master/examples/wgpu.rs) uses wgpu instead of egl, so egl might not fit my use case.
I am not too worried about integration, since I just need to modify a slice.

All the ash examples I look at use winit, which begs the question of why I chose wayland client over winit.
I might need to migrate to winit later on.

2024/10/02

I looked further into zed, and they use a crate called blade graphics.
It isn't very widely used, but I guess the creators of zed like it.

I think I need to start considering crates with not a lot of downloads.
There is a crate called `ocl`, and I instantly love how simple their example is.
The repo was last updated 6 months ago, so I am not entirely sure if it is 
`rust-gpu` is even simpler, and it actually somehow works in pure rust, which is very cool.
It uses something called spirv, which adds an extra layer of complexity to the code.
Rust gpu also currently doesn't have a crate.io page, which is kind of weird.

I am going to try ocl. 

After a lot of fiddling with nix, `hardware.opengl.extraPackages = with pkgs; [ intel-ocl ];` in my configs.nix is what finally fixed my problem.

Make sure `clinfo` says number of platforms is at least 1 if you are also having issues.

---

The simplest way of sending the element data to the gpu is probably by having a different rendering function per each element primitive (text, rect, ...) and passing the remaining arguments (coords, color, ...) via parameters.
Later on when I do multiple elements, I might just render each element and let elements render on top of each other.

My shader code is most definitely suboptimal, but it is a good experience probably.

Wow, I don't understand half of what I just "wrote" but it was surprisingly not as hard as I thought.
The problem right now is that I don't understand the types and how to pass data to the gpu.
To fix this, I will read [this tutorial on OpenCL](https://www.nersc.gov/assets/pubs_presos/MattsonTutorialSC14.pdf).
This is what I gathered from the tutorial as well as the example code from ocl.
- Levels of stuff (pg 11):
  - Host calls the compute device, which consists of multiple work groups, which themselves consist of multiple work items. Each work item calls the kernel function once.
  - Levels of memory:
    - Host memory
    - Global and constant memory: shared within the entire compute device
    - Local memory: shared within a work group
    - Private memory: individual for each work item
    - This is pretty helpful, so I guess inputs marked as `__private` are individual for each work item, and the same thing for the other levels.
- The cl kernel code starts at pg 41.
- Vectors (eg `int4` is 4 integers, more like const sized arrays than rust vecs)
  - I needed a seperate auxillary crate `ocl-core-vector` to send vectors as args
- NOTE: `float` is rust f32
- NOTE: when a type is marked with `*`, it is actually just a pointer so it would be equivalent to a rust `&` I think
- `get_global_id(0)` returns the first work id, which is the x pixel in ocl. 1 is y.
- NOTE: Ocl's `ImageChannelDataType::UnormInt8` is a float from 0 to 1, not an integer, to use u8 is: `ImageChannelDataType::UnsignedInt8`

[Here](https://registry.khronos.org/OpenCL/specs/3.0-unified/html/OpenCL_C.html) is the documentation for open cl.

---

Idk how I would start displaying fonts, I guess I should start with more research.

https://en.wikipedia.org/wiki/Computer_font
- Three basic ways to store each glyph:
  - Bitmap
    - Matrix of pixels
    - Feels jank even for my standards
  - Vector/outline
    - Store instructions on how to draw, like bezier curves
    - I don't want to manually do all the instructions for this
  - Stroke
    - Store a series of strokes (and possibly other info)

I actually don't want to do fonts myself.

`ab_glyph` and `rusttype` both seem  to meet my needs.
It looks like ab glyph made rusttype obsolete, so I guess I am going with ab glyph.\

Actually, I could use ab glyph's rasterizer to support the gpu.

Okay, it seems pretty simple, but it is a little annoying that I need a .otf file in the project to do this.

font_kit, the crate I was previously using, might actually work, and it doesn't require me to include a font with the project.
It is tightly coupled with the pathfinder crate, which makes sense because both crates are developed by the servo project.

2024/10/03

If I need to import a font, I'll go with DejaVu fonts because it is public domain.

---

I got ab_glyphs to work, but my implementation isn't super fast.
Of course, loading takes time, but even for just the drawing portion:
```rust
q.draw(|x, y, c| {
    *img.get_pixel_mut(x + 10, y + 10) =
        Rgba([(c * 255.) as u8, 0, 0, u8::MAX]);
});
```
drawing a 12pt character onto the picture takes around 20µs.
For a glyph, raqote (criticized for its speed) took around 5-10µs.

As for the gpu renders, building a kernel took around 150µs and executing it took around 20µs.
Raqote took around 250µs for rectangles before optimizations
So, I hypothesize that the more things I can do with a single call to the gpu, the more effecient it will get compared to the cpu.

Okay, knowing all this, I am going to put a hold on the GPU stuff, because it wasn't as fun as I thought it would be.

---

2024/10/03

I think that the best thing now is to just implement the other subapps.

Once I do that, I can start the project/workspace wise features.

### Hierarchy Operations

2024/10/08

I have some ideas for moving tabs in the hierarchy.
I think the easiest way is to "<u>P</u>luck" trees into a temp buffer, then "<u>P</u>lace" them somewhere else.
BTW, I am going to be using Alt for most (or all) the hierarchy operations, because that is how window switching works in other OS's (Alt+Tab).
There can also be a mark operation to allow for things like swapping two tabs.

Before that, I should actually do tab closing first.
The one tricky decision with closing is the tree hierarchy.
Ideally, the tree (specifically the deleted node's children) would maintain its rough structure when a node is deleted, and a parent node can be deleted without deleting the children.
The two basic ways of doing this would be:

1. Put all the (direct) children of the deleted node in the 
   1. If thinking connections-wise, equivalent to just connecting deleted node's parent directly to deleted node's children.
2. Make the first child take the place of the deleted node, and the other children don't move so they are now the children of the first child
   1. Kind of similar to the heap deletion algorithm.

I will first implement a full deletion first, but I think it will be best to implement trees specific to my goals.

2024/10/10

I am implementing a tree based of id's, but something I might want to change later is the fact that the id tree is heavily coupled with `Tabs`.
It is like the rooted tree, but it just uses IDs as indices.

Deletion is actually quite hard, on second thought, I will not do hierarchy operations right now.
(Ex: removing a non-last sibling requires all later siblings and their children to redo their paths, I might end up needing a different system of storage entirely. The fact that children are ordered is a big obstacle)

## Organization

2024/10/10

I was thinking about my vision for singularity, and I think this is the order of "categories" that makes the most sense to me (think of this like the Kingdom-P-C-O-Family-Genus-Species):

1. User
   1. The file storing the user data could be a special case of the project storing file, maybe with a special `userfile=true`
   2. But, there could be shared projects later on
2. Project
3. Custom User-set hierarchy between tabs

This is similar to how in nix, there is a configuration.nix which is for the user (or the machine), but when a user enters a nix-shell or dev flake, the packages/features available are a union of the packages from configuration.nix and shell.nix.
Actually, now that I think about it, most apps are like this.
As another example, VSCode has User Settings and Project Settings.

2024/10/11

In order to code faster, I am going to ignore the permission stuff for tabs, which seems like bad foreshadowing.
Additionally, I will ignore things like abstraction, since the main priority is just the task organizer.

2024/10/16

I want to be able to standardize children elements (eg: components or widgets) for things like "forwarding" events (especially mouse-clicks) and possibly focus.
Planned steps:

1. Make a trait for elements
   1. `Tab` kind of works for this, but something else might be better
2. Create forwarding method
   1. Only difference right now between `forward_event` and `handle_event` should be that `forward_event` somehow should take care of mapping mouse

2024/10/17

Ugh, I don't know how I want to do this...
I might somehow make use of Mutexes of UIElements.

Actually, it was pretty easy, barely an inconvenience.
I did it with a new trait called `Component`

Things I could implement/consider for the components stuff:
- [ ] focus (should I even do this?)
  - [ ] either make this an event (this is probably the way most other things do this), or give `focused` as a parameter for each render, or both
  - [ ] implement setting focus
  - [ ] implement different display for focused vs unfocused
- [ ] vec of components as a component (like UIElement::Container)
  - [ ] Somehow make this like map tuples and stuff, (like being able to write `Components<(A, B, C)>` and it works like a vec for Components, but it works like a tuple from the outside (you can call `component_bundle.0` and the compiler knows it is type `A`))
    - [ ] Might be able to do this inductively, inspired by: [tuple tricks crate](https://crates.io/crates/tuple_tricks)
    - [ ] Recursive Impls are probably better, it is also in the [std library](https://doc.rust-lang.org/src/core/fmt/mod.rs.html#2628)
      - [ ] Same thing, but [simple example](https://stackoverflow.com/questions/55553281/is-it-possible-to-automatically-implement-a-trait-for-any-tuple-that-is-made-up)
  - [ ] get focus working with this
- [ ] make the components with inner components less bulky to write
  - [ ] maybe mutexes might help?

2024/10/24

I don't like the way this is currently implemented, but since I am using macros anyways, I might look into having macros instead of using the complex types.

I want to reattempt tab structure operations.
My first change is going to have the tree path be not order based, but id based.
This means I will need to reimplement all the tree traversal logic and very tightly couple the tree logic with the tab logic, but I am willing to make that sacrifice.

2024/10/25

Btw, I ended up having tree logic decoupled from all the tab stuff.

Anyways, I am making procedural macros, and it was kind of annoying to start because there was not great documentation, but it feels pretty simple once you get started.
Here are my tips:

- My main source for getting started was a [video](https://youtu.be/crWfcA064is?si=AbTf290vzLE4bhR0) by Let's Get Rusty.
- Use `cargo expand`
- [this article](https://www.freecodecamp.org/news/procedural-macros-in-rust/) also seems good and very in-depth
- use the `quote` macro


2024/10/27

I've been going on tangents from the `Organization` subsection, but I will get back on topic now.
The thing I was supposed to improve was the `.project` stuff, with an initial focus on tasks.
I think I will just start "drawing inspiration" from similar existing tools like VSCode and Nixos.

First, I want to improve open/close behavior:
- [x] be able to run and specify project path in cli, something like `singularity_manager --project examples/root-project`
- [x] save the workspace's open tabs on close

2024/10/28

I am working on saving tab sessions on close, but how should the data transfer happen?
I could just do something rather---contrived, if I only cared about this, but I would be avoiding the overarching matter of how information should travel between the tabs and the project.
And in a way, this brings into question what singularity itself is.

I initially thought singularity should simply be an app to host other apps.
I still want an app that does that.
But after inspiration from Wayland, I believe that an interface protocol between subapps/tabs and a centralized app can be much more powerful.

Anyways, that is something I should keep in the back of my mind while I continue making progress.

I think I can draw inspiration from webpages for saving data.
These are the aspects of webpages I think are relavent:
- Information for opening a webpage:
  - The webpage location in the url (most of the url)
    - Parallel the "tab_type"
  - Extra parameters in the url (like the `url.link/page?parameter`)
    - Examples of this are in many search pages and also when specifying sections (wikipedia)
    - Data from the opener to the tab on initialization
    - Should also remember/ask for this when saving a session, if you want to restore it later
    - Already implemented this for opening, but I should also add it for closing
  - Cookies/local storage/session storage
    - Local storage
      - Data per tab type (and per .project type)
      - I think I can add this with queries
    - Session storage
      - Data per instance of tab
      - This is just variables, already implemented

2024/10/29

I can add initialization parameters and something like local storage through queries.
In essence, the initialization parameters are storing the session storage long term, from one close to the next init.

Both extra parameters and local storage will not be generic types that are different per tab type.
Instead, I will pull a javascript and just store a serde value or even just a string.

I think there is a reason why webpages don't save data on closing like this (that I know of, +other than caching).
Saving on close is a pain, especially with the current architecture with threads.
Keeping with the webpage inspiration/theme, I think I will just save the initialization data throughout, and save it on close.
Later on, I can modify that code slightly to have a more general tab data.
Wait, I think I just reinvented session storage.
(As much as I like to hate on JS and webdev standards, I am slowly recreating it with this project...)

Though I could use Mutex for session storage (and later might), I will do it with queries for now.
If I do that, the current infastructure for UIDisplay can be reused for session storage.

I got expected behavior first try!
However, I still need to find a way to figure out the tab type.
In fact, I am not sure how I am going to handle tab type at all.
But, I am ready to sleep now.

2024/11/1

Coder's worst nightmare, naming variables, strikes once again.
I've been stuck working on automatically generating code for queries.
I am trying to procedurally name variables, but that has turned out to be quite a hassle.

Concat idents is unstable, and while the paste crate seems cool, but is deprecated for some reason.
I tried to do my own implementation of the concat_idents macro, but it sometimes just doesn't work, so I will try using paste even though it is deprecated.

Using paste, I automated generating the query type stuff, but it is not very readable, and I am also worried about its performance.
Additionally, if something does go wrong, it will be a nightnmare to debug.
I haven't used the auto-generated queries yet.

Uhm, this feels noticably slower, I can't remember if it is because of my new changes though.

2024/11/2

I think that was just because I had so many tabs open from the open/close session test.

2024/11/3

I tried to do wayland embedding in another branch, but that didn't work out.

Anyways, I want to work on organization in the form of tiling.
I will ~~copy~~ gain inspiration from existing tiling window managers like hyprland.

The problem is that a lot of the keybinds I want to use are already being used by KDE.
I am going to disable them for my entire system, because there is no way to selectively change KDE keybinds based on app focus.

2024/11/4

I talked to a friend (@glolichen) who uses Hyprland, and it seems like everything can be stored in a binary tree, with leaves being the actual windows, and non-leaf nodes storing two children, and data about how they are organized (eg, hor vs vert split, split ratio).

2024/11/6

I am once again turning to Uuids for the binary trees, and I realized I wanted type safety with uuids, but the rust compiler is kind of annoying so I might use an external crate for this.
I looked at the `typed_id` crate, and it just manually implements what would have been derived.
But, I feel like there is something wrong with my logic as I rely more and more on Uuid's.

2024/11/7

I learned that hyprland has two ways of doing tiling: dwindle and master.
Dwindle is what I was basing my tiling off of.

2024/11/11 (technically 2024/11/12 1:09 AM)

Plan:
- [x] render based on focus state (boolean) (this sets up tree elements ui to change highlights on select)
  - ideas
    - pass `focused` boolean on render (idk, feels kinda jank)
    - pass render_data on render (a more abstract version of the `focused` idea)
    - use macros (this will be the hardest, but I want to do it)
- [x] some abstraction for tree displaying
- [ ] improve task organizer

2024/11/12

I think I was overcomplicating it.
For tab focus, I need to standardize it, but with widgets, I might just have it be different for each.
If I really wanted some standardization for widget focus, I could just add a trait.

For the tree displaying thing, I will use macros.

2024/11/13

I think I can just add it to compose components.

First, I implemented the logic without macros, then, in demo, I wrote what I wanted the macro use to look like, and then I implemented macros while changing the macro use to fit logistical constraints.

(warning: the demo window has an empty background, and at first, it looks like it isn't running)

2024/11/16

I was using enclosed component along with compose component for the focus task editor, so there might be leftover redundant code from doing that.

2024/11/17

I tried to use lldb in VsCode, but unfortunately it requires extra setup in nix which doesn't seem worth my current time.
Plus, it probably would have not worked with multithreading anyways.

2024/11/18

Bruh, why did I never find [this video](https://www.youtube.com/watch?v=Api6dFMlxAA)?
The title isn't really descriptive but it is a video on tiling wm algorithms.
Things I want to look for:
- Moving tiles around
  - (Representing tile structure and adding and deleting tiles are already implemented and are pretty intuitive)
  - The current swapping system is limited
- Inspiration for tab hierarchy as well if possible

Notes:

- 5:32 starts talking about approches to tiling
  - 6:14 list-based
    - 6:35 stack mode/master mode
      - One master takes up half of the screen
      - All others are just stacked on top of each other
      - Lame (imo)
    - 8:20 Max/ful/lmonocle
      - Bruh
      - Sucks
      - Only full screen
    - Continues talking about other stuff, but as expected, all the list-based methods are cringe
  - 11:45 Tree-based
    - The example's tree structure is similar to what I use and has parents being containers while leaves are actual windows (tiles in my case), but it is not a binary tree
    - I didn't watch the whole thing, but unfortunately, it seems the slides only go over adding tiles so it doesn't really help me

I tried to look elsewhere for similar sources, but could not find anything.

For moving tabs within the tab hierarchy, I think I have a way:
I already have tab hierarchy traversal, where given the current tab path and a keybind (Alt+some char), I get a new tab path.
I can have a corresponding keybind for each traversal (Mod+Alt+same char) (I wanted to do Alt+Shift, but that would make things harder bc it would modify the character) where the move keybind just swaps the current path's tab and the new path's tab in the tab hierarchy.

Actually, while implementing the above method, I realized that there was already an easy way of implementing this.
There was already a selecting screen, so algorithms like pluck and place as well as swapping will be obvious.
But, I will first implement and commit the other way because of sunk cost fallacy.

Gee, I am really struggling with swapping two nodes in a doubly-linked tree (doubly linked referring to the fact that parents and children both link to each other)...
When they are unrelated or are siblings, everything works normally, but with parent-child, everything breaks apart.
I might need to go back to baisics by considering how to swap a doubly-linked list.

2024/11/19

I just drew the connection diagrams on a whiteboard and tried algorithms until I found one that worked:
1. Update all the children connections (for all children connections, replace A and B)
   1. Swap the children A and B for their parents
   2. Swap A and B's children
2. Update all parent connections to match the children connections

My next goal is to get the Task Organizer to a very good point.
I will brainstorm now:

Though have left Java and many of its ideals of OOP long ago, I believe that approaching the Task Organizer in an MVVM (model, view, view-model) approach might be an elegant way of thinking about it.

- Model
  - The first component of MVVM is the model, which refers to how the data is represented.
  - In the case of the Task Organizer, the model will store the tasks and all relevant data.
  - Most of the model can change as I work on the view and the view-model, but an important task regarding the model is file representation.
    - So far, I overlooked the file representation of everything by making rust structures and using Serde to convert data into JSON.
    - For now, there are more urgent matters, but I ultimately want people to be able to edit these files manually with a text editor.
    - Until then, the graphical editor will suffice.
  - Model could also include some logic, but I will be referring to only the representation of the data as the model.
- View
  - The view refers to the UI, including displaying data to the user and recieving user input.
- View-Model
  - The View-Model can be thought of as the connection between the View and the Model.
  - A critical aspect of the View-Model's responsibility is modifying data.

Now that I wrote all that, I am not exactly sure why I did, but I guess it is a good reminder.

Features I want to work on:

- Time management
  - Regulate and log time spent on each task
  - Different from deadlines but I should do that later as well
  - Regulate time with blocking method
  - Log time by recording all timer related actions along with when that action happened
- Templates/repeating task types
- Links
- Online webpage access

Blocking method idea:
I haven't tested this so I don't know if it is actually efficient, but the idea is to work on a task for specific blocks at a time.
A block is a continuous period of time dedicated to a single purpose (like working on a task).
A standard block should have a set-up, work, and clean-up period.
The setup would include setting up prerequisites for the work, finding a good playlist, using the bathroom, clearing the mind of all else (ex: check gmail to not worry about missing emails), and setting up the task and block in the task organizer.
During the setup period, make the work period as smooth and productive as possible.
Cool down during the clean-up period, and take care of the physical workspace so that whatever comes next can be started in a fresh state.
For an hour-long block, a good delegation of times could be a 5 min, 50 min, and 5 min, respectively.
Unfortunately, school and other obligations exist, so I can not divide my life into perfect 1-hour blocks.
I also must consider the fact that the time it takes most tasks (often tasks with objective requirements) can not be perfectly predicted, let alone fit into 50 minutes.

Actually, I am going to just test different ways of being productive and try something new each time I don't like something.

### Trials

Control Trial

For my first trial with tasks, I will start with blocks and a timer that shows overall time spent on each task and logs each start and stop time (can only be viewed from the JSON, meant for future data analysis or debugging) with no time limits or target times.

Future features that I want to test in trials that are not in this trial:
- Reusable block and task templates
  - Ex of block template:
    - Set-up, work, and clean-up
- Pacer
  - Flexible target time
- Try seperating blocks and tasks entirely
  - Task can be analogous to GH issue and block can be analogous to a commit
  - Blocks don't necessarily need to be under a task, but blocks can reference tasks in descriptions (think how descriptions will work)
  - If this test produces fruitful results, could later even have a different app (tab) just for blocks, where references between each are allowed
  - Sub feature to test: descriptions that are like commit descriptions
    - Could have something like a small devlog accompany each block (would kill 2 birds with 1 stone bc I wouldn't need to build a whole new app for devlog and I can save redundant time that would be spent on writing devlogs bc it automatically stores it)
    - Can reference tasks and other stuff (much later, I should standardize linking)
    - Can also have special regions for the description or just templates for descriptions
      - Like a goal section and reflection section
    - Having this will allow users to work on multiple related tasks in one block, and this might be bad if strictness is desired, but I hope it will strike a balance between flow and structure, because it all
      - Having subblocks would be better organizationally, but that is getting out of hand, even for me (if I did this, it would be supplimentary to block descriptions)
- Rigid predefined blocks
  - Set target time limit before each block, and the block must end when the timer goes off
  - If you don't finish by then, start a new block either right afterwards or after doing somthing else
  - I don't really like this

I want to try all these in different branches to test all seperately, but I also don't want to bloat up the number of branches.
Do I make a fork? Can github issues or some other GH feature help me? I guess the feature I am looking for is literally just branches.

Features to add after all the trials are done:
- Blocks report
  - Overview of task worked on at each point in the day per project
  - Have this work with nested projects as well
  - Have a complete overview for all projects for a single person
    - Can also have a special thing to note non-project blocks like sleeping and eating

Many of the features promote data collection at the sake of short term flow, and I am actually fine with that because it means I can continuously learn from this data and improve singularity, and I also predict that being able to visualize productivity will help gamify productiveness.
TBH, I just like seeing data.
I am not worried about privacy, because 1: I might be the only one who uses singularity, 2: this is all locally run, 3: I can add opt-out/in and encryption later if needed.

By the end of the trials, I want to be able to use singularity because it is good.

2024/11/20 12:13AM

NOOOOOO, I thought of the name `chronoschism` meaning time (`chrono`) and divide (`schism`), but it turns out that someone else came up with it first!!!!
Grrr...

I am going to use that name though because I came up with it seperately and it is cool.

Also, I was thinking about the trials, and I realized I am not going to do that in the scientist way; instead of testing all the variables seperately, I am going to use an iterative approach and just use common sense.

2024/11/21

I am going to make a seperate subapp for blocks, called time manager.
The name chronoschism would be better for a name of an idea or game, but it doesn't feel like it should be a name of a tool.

2024/11/23

I had to somehow start a time manager tab, but I don't have a way of starting tabs (TODO), so I temporarily made tabs public and inside the manager's testing code, I made it add a new time manager, then ran it once, and removed it.

2024/11/25

Should making borders around each tab be a responsibility of the tab or the manager?
Right now, I am putting borders around each tab's inner elements in the tab's code.

2024/11/26

Ideas relating to what I am implementing right now (the time manager/the block thingy):
- Log every action saved
  - Like git but it commits when you save (every keystroke is crazy even for me), modify tasks, execute cli command (like a more verbose/intrusive version of `history`), etc
  - Possible rule of thumb: at least every time "data" is modified
  - PRO: Would help automate blocks
    - Instead of starting a timer, doing stuff, and stopping, you could just do a bunch of stuff, and the time manager automatically clusters into groups, like "N saves, cli commands, and etc all with a maximum of a T minute gap between each consecutive action? sounds like a block just happened from x to z O'clock"
  - PRO: Would also be useful if connected to internet
    - Allow for backups
    - Like a middle ground between google docs and git commits (just me, or does anyone else get git commit anxiety, like a feeling of paranoia that a commit is too small: "oh no, people are gonna think I am just farming my github statistics" or simply not wanting to name an insignificant change so I sneak it in with a major change under the name of that change)
      - Google docs reminds me that a good alternative name for "block" is "session"
    - Should figure out how to make this work with git
      - Maybe all the block stuff (and even the .project directory) should be .gitignore'd, especially considering multiple branches or multiple collaborators
    - Connect to self-hosted server?
      - Would be jank, but highly customizable, could have a web interface so it is accessible from my phone
  - PRO: yay data
  - CON: Too much data?
    - Hard to store
    - Intrusive
  - would have to actually implement it (side effects)
    - there would need to be some standardized system for 
      - good bc it could potentially be useful for other stuff
      - bad bc sounds like a pain
    - Customizable user scripts for clustering also sounds fun to use but like a pain to make
- Block notes and title
  - When should I let the user edit this?
    - Before starting and during the block
    - Do I let the user edit afterwards?
  - Notes can be optional
  - Title defaults to "Block N" for the N-th block
- Linking between tabs
  - To quote Kylo Ren: "I know what I have to do but I don't know if I have the strength to do it."
  - Prerequesite to this would be some standardization to the list of 
  - Implementation idea based on the web:
    - Each tab type is like a server, imagine task-organizer.net and time-manager.net
    - That is, there are tab-wise (server-wise)
    - Each instance of a tab is like a webpage session
    - Ex workflow: If an open time manager tab wants to focus a specific task by id, then the task organizer would create a task-organizer packet object with the relevant data (probably an enum like `Packet::FocusTask(task_id)`), serialize it into a json object (or some other standardized but versatile datatype), then tell the manager to forward the json packet to the task organizer tab type/"server" (figure out how to identify tab types, ideally something better than just mapping string to impls of some tab trait). On the "server"-side, somehow try to see if there is already a task-organizer tab open. If so, then somehow identify it and tell it to focus on the relevant task. If there are no task organizer tabs open, then ask manager to create one with the focus on the relevant task.

2024/11/30

I have previously expressed discontent towards my current procedural macro implementation for components.
I want the macro to be more versatile, I am thinking the user writes something like:

```rust
#[derive(Components)]
struct Tab {
    component_field!(some_component_name: String; 
        clickable,
        area = DisplayArea::FULL,
        render = UIElement::CharGrid(CharGrid::from(self.some_component_name)),
        event_handle = { ... },
    ),
}
```
which gets expanded to
```rust
struct Tab {
    some_component_name: String,
    /// Generated by macro `component_field` and not to be used directly
    __some_component_name_clicked: bool,
}
#[automatically_generated]
impl Tab {
    fn some_component_name_clicked(&mut self) -> bool { ... }
    fn render_some_component_name(&mut self, manager_handler: &ManagerHandler) { ... }
    fn handle_event_some_component_name(&mut self, event: Event, manager_handler: &ManagerHandler) { ... }
}
```

The macro to generate more fields was cool, but unnecessary.

Actually, I can just have this, which in terms of the macro formatting, is very similar to the current system (that is to say, the difference is in what it gets turned into):

```rust
#[derive(Components)]
struct Tab {
    #[component(area = DisplayArea::FULL)]
    some_component_name: String,
}
```
which gets expanded to
```rust
...

#[automatically_generated]
impl Tab {
    fn remap_event_some_component_name(event: Event) -> Option<Event> { ... }
    fn contain_some_component_name_render(render: UIElement) -> UIElement { ... }
}
```

hmmm...
I don't want to say this, but how much of this is actually necessary?

Let me take a step back:
Once the excess (uselessly rigid) parts of the macro are removed, it seems all I am doing with the macro is ensuring that clicking and rendering are remapped by the same amount.
And I only do this because I want components, tabs (tabs are slightly different), elements, etc to be allowed to be unaware of the context it exists inside.
So far, context really just means its display area, and the two things impacted by display area is clicking and rendering.
Even its pixel size doesn't need to be known for most purposes because of the relative unit.
There is also focus, but I didn't really implement that, so it doesn't count right now.
Anyways, this means that the burden of contextualizing lies on the container (the user of a component).
This approach will allow components to do the bare minimum, but might create a lot of bloat on the container side.

The meta purpose of the `Components` proc macro (which I might call the `ComponentsContainer`) is to create a flexible tool to help simplify as many container usecases as possible.

Honestly, I might just make containers manually do this like:

```rust
struct Tab {
    some_component_name: String,
}
impl Tab {
    const SOME_COMPONENT_NAME_AREA: DisplayArea = DisplayArea::FULL;

    fn render(&mut self) -> UIElement {
        { todo!() }.contained(Self::SOME_COMPONENT_NAME_AREA)
    }

    fn handle_event(&mut self, event: Event) {
        if let Some(some_component_remapped_event) = event.remap(Self::SOME_COMOPONENT_NAME_AREA) {
            todo!()
        }
    }
}
```

This is very boring but solid.
On the plus side, I should be able to do this already.
I will deprecate the `ComposeComponents` macro for now, RIP ComposeComponents.

Button also seems unnecessary, but I will tackle one problem at a time.

...

I removed the compose components macro from time manager, but I am noticing a few patterns. 
In fact, I might venture to call parts of it "repetitive" or even "automatable".
In case the file has been changed and you don't know what I mean, this is the github permalink:

https://github.com/mathkimchi/singularity/blob/0d16c4488b81f92a99c49e59d14d46202824f3e5/singularity_standard_tabs/src/time_manager/mod.rs#L213-L246

(speaking of cool github features, I should start using GH issues and pull-requests; In the 2024/11/19 DEVLOG entry, I allude to wanting a feature that works just like this)

Well, just look at these three specific snippets in the same file:

```rust
Focus::Title => Self::TITLE_EDITOR_AREA,
Focus::Body => Self::BODY_EDITOR_AREA,
Focus::Timer => Self::TIMER_BUTTON_AREA,
``` 

```rust
else if let Some(remapped_event) = event.remap(Self::TITLE_EDITOR_AREA) {
    self.focus = Focus::Title;
    remapped_event
} else if let Some(remapped_event) = event.remap(Self::BODY_EDITOR_AREA) {
    self.focus = Focus::Body;
    remapped_event
} else if let Some(remapped_event) = event.remap(Self::TIMER_BUTTON_AREA) {
    self.focus = Focus::Timer;
    remapped_event
}
```

```rust
Focus::Title => {
    self.title_editor.handle_event(remapped_event);
}
Focus::Body => {
    self.body_editor.handle_event(remapped_event);
}
Focus::Timer => {
    self.start_button.handle_event(remapped_event);
}
```

There is a little voice telling me this is going to be not as good as I think, that I am overcomplicating this, but a meta macro could work.

The big observation is that there are distinct triplets of a `Focus` variant, a field, and a DisplayArea.

You know what, I am going to leave it at the observation for now, because I want to make fast changes today.
This is definitely a TODO or REVIEW though.

TODO

2024/12/2

Do you see this?
I finally cleaned up the git branches and got around to using issues.

From now on, I will put the technical brainstorming and ideas in github issues.
In this file, until I actually deem singularity (specifically the time manager's block logging) good enough to actually start using, I will mostly be writing session-related things in here.
This will be things like:
- Writing down/logging issues I work on like I will right below
- Journalling about the coding process, like writing down how I got over a specific problem, or if I didn't get over a problem, just ranting about it
- Whatever miscellaneous things I feel like writing here

The purpose of doing this is so there is a central place to see everything I did.

I will work on this issue (hopefully github recognizes this):

#3

...

Nope, but this should work: I will work on [#3](https://github.com/mathkimchi/singularity/issues/3) (apparently vscode recognizes these issues now, because it autocompleted).

...

Beware: Rambling (even more so than usual)

My roommate is sleeping and I wanted to log my thoughts before I wash them away in slumber, so I have resorted to editing markdown on github mobile.
I was thinking of having each process create as many windows as they wanted, if any.
But, instinctually, I worry that jumping to this solution might not be the broghtest idea.
I keep thinking of rust's ownership, not necessarily because the its solution to memory management might cleanly parallel a good solution to this problem, but because it is just so clever that I want to know how that idea was even conceived.
In other words, I want to try to use the example of ownership to learn how to come up with good solutions.
Like manual memory management, my idea pretty much maximizes the flexibility of how tabs are implemented.
Flexibility is great, but in this case, making tabs too flexible will make it unsafe and hard to develop, like C.
Moreover, learning about wayland has shown me that imposing logically arbitrary but practically useful works, which is reassuring especially since I intend to implement a system with the same functionality as wayland.
(However, learning anout wayland has also taught me that it is a nightmare, so maybe it isn't the best role model.)

The thought that initially motivated this was rust's mpsc.
A rust message channel can have multiple senders and one receiver.
There can only be one receiver, because the message can only have one owner.
This is very intuitive if we think of a real mail.
A channel is like a mailing address; there is no limit to how many people can send messages, but each mailing address has one house it refers to.
Rust ownership makes more sense than loose memory management in the real world, even though following it sets a self imposed restriction that isn't necessary by the nature of where code lives.

I wonder if I can draw any inspiration from real life to come up with a good solution.
The takeaway from the above paragraphs is that the inspiration can be arbitrary from a purely logical view (in other words: restrict what the user (a developer using this code) can do even though allowing it would not be hard to implement), if it benefits safety or improves the usecases that will be allowed.
I guess all that just says that guardrails are fine.

2024/12/4

I haven't found a better solution than processes requesting to make windows.
This is the most flexible way.

But, I was thinking more and eventually, I should abstract away everything arbitrary, and for any feature like mouse click listening, have it work similarly to importing libraries.

On account of my desire to open source this project by the end of December combined with my inability to dedicate myself wholly to this project at this moment in time due to urgent priorities (studying), I will continue to brainstorm until Saturday.
On Saturday, I start implementing the least bad solution, even if it is the flexible way.
This will let me:

1. Prioritize my studies for the week, which is the most crucial period
2. Brainstorm and ensure there are no glaring issues with whatever I settle on, so I probably don't prematurely work on a fundamentally flawed implementation (which I have done)
3. Not get stuck on brainstorming, constantly doubting any solution I might do, and never start trying (which I have also done)

2024/12/7

A terminal app called Zellij is written in rust and supports rust plugins, but it is with WASM.
Check out: https://zellij.dev/tutorials/developing-a-rust-plugin/

Anyways, my deadline is up.

I thought about the best ways of doing this, and I didn't get any big revelations.
I will take it simple and do things similarly to window managers and desktop apps: each process can request an arbitrary amount of windows.

I think I will have a singularity client toolkit.
This is similar to the smithay client toolkit, and it is my attempt at making things safe when safety isn't guranteed with ipc.
In theory, if the client toolkit and the server side are both working, then the actual client code should be safe.

There is a [smithay handbook](https://smithay.github.io/book/client/general/intro.html) that I will gain inspiration from.
In wayland, the wayland server has a listener at `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`, and if you `echo $XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`, you should get something like `/run/user/1000/wayland-0`.
So, I will have my own env variable called `$SINGULARITY_SERVER`, and the singularity server will make a socket at `$XDG_RUNTIME_DIR/$SINGULARITY_SERVER`.

When the wayland client wants to make a window, they call `Connection::connect_to_env()`.
Each `Connection` should represent one wayland window.
I'll do something similar.

2024/12/13

I haven't worked on this over the week, but it is the weekend, and I want a demo just for the unix sockets.
I will make a chatting app to start simple with.

I also think I should seperate singularity into:

- singularity tab organizer (what I created this project with the intent of making)
- singularity window manager/compositor
- individual standard tabs
  - Chro (name in progress), the time manager
  - rest of the tabs (file manager, text editor, eg.) are whatever, just make them usable

unix-stream seems to be the same for the server and client side, the process just seems to be:
1. server makes a `UnixListener` (this is only for server side)
2. client tries to connect
3. server accepts, and the OS or whatever gives both the server and client each a `UnixSocket` that just communicates between the two

I remember making a multiplayer game with TCP, and I think this is very similar.

A group chat server is unnecessarily complicated, I will just do a simple turn based chat like thing between just client and server.

2024/12/14

Now that I have the communication sockets figured out, I should actually use it to allow tabs to run in processes.
Since I am pretty much making a window manager, I will use the term `app` from now.
There should be a one to one mapping between an instance of an app and a connection.

The singularity app protocol (sap ?) should support extensible features.
What I mean is that, if the server and client both want to support dragging as an optional feature, then they should be able to send dragging packets to each other.
But, if either of them don't know dragging, then dragging will not work, but the app should still work.

I can't think of a way to make this work well at runtime or to ensure complete safety, but here is my idea:

- Each feature (including the standard features) can be a crate or something
  - Has a unique feature id of a constant size (like u64)
  - Should have exactly one type each for `Event` and `Request`
    - These should all be serializable to and deserializable from binary (`&[u8]`)
    - These are the packets currently inside `singularity_common::tab::packets`
    - I am grouping `Query` into `Request`, but idk how to do type safe responses right now.
  - I think there is an error thing like this
- If a message with binary b of feature id f is to be sent, then send the tuple (f, b).

I was talking to a friend (@glolichen), and they suggested sharing a file that lists all the features that will be used.
Maybe I could send an initial message just to say what features each side supports.

2024/12/31

OK so kind of liking off on singularity if you couldn't tell but I'd stuff up but I'm gonna work on singularity now hopefully. Enter line
But anyways it's like 2 AM and my sleep is all messed up anyways so it's actually pretty early for me like I've been sleeping much later than this but anyways, I'm trying to sleep early today because I have to wake up early tomorrow, but I couldn't sleep just keeping this in my head because I was thinking about singularity instead of trying to sleep.
So I decided I'm just gonna write down the idea I had using voice memo on my phone for like voice dictation. You know what I mean
so the idea was anyways I've been thinking about I've been thinking about how to actually do like responses and stuff because it's kind of complicated to do it especially with multiple threads and now it's even multiple processes and before I go over my my current idea, I'm gonna try to go over the other ideas I had first so the first idea was not the first idea, but one of the ideas was to just create a new connection each time so if I wanted to get a response on like that, the overall window size in pixels then I would like somehow create a new response or a new new like form of process communication like maybe I could share some memory just to represent the response for that and then once that response, it's actually fulfilled then it would be useless and that is I think that would be the safest way so far but it's quite evident why that is not the best idea and the other way I was thinking was I was looking at how Wayland does just process communication between the client which are like the windows and the server which would be your your Wayland compositor and as looking at Wikipedia and the dogs and I actually also ask ChatGPT cause I couldn't really find a straight answer in the in the official documentation. I'm sure it's in there somewhere but I asked just how does Wayland manage like requests from the client that are expecting responses in my in singularity I'm gonna call these types of and chat. GPT told me I'm not sure if it's accurate but it makes sense so that's all I really need what it told me was that every request would have a unique ID associated with it and then for every every query would have ID associated with it and the response would somehow include the query ID in it so that we know exactly what the response is a response to. So and then.
OK, the app decided to crash on me, but I'll try to pick up where I left off. I was talking about the ChatGPT solution so it would have a unique ID for each request and then the response would somehow represent that it would somehow include that request ID and ChatGPT mention that it also included time data but that not like fundamentally necessary so I'm gonna ignore that for now and everything else that I've already implemented like the the type ID so the ID for actually like what kind of thing this is as well as the as well as the message length ID are actually things in Wayland already I kind of expected the type ID but I was actually surprised that the message length ID was something well ended. Maybe I copied it and then forgot about it but anyways the request ID seems like a not horrible ID for it but the only the only problem I have right now with that would be that there's gonna be multiple threats on the server side and I don't really care about the multiple threats on the client side that doesn't really matter but for example, what if the client sends a query but before the server sends the response, it sends just a event to the client so it's on the client receiver side. It's gonna get the event and then the response and then I doubt there's ways you could try to solve this, but it's all quite tricky and there there is bound to be a better way so I was thinking I could still do the response and request ID idea, but the only difference would be I would have a rapper for it so my idea for the rapper would be something like the universal client socket or something like that for the client and then for the server, it would be the universal server socket and I already have something called like universal packet socket or something like that or like universal packet writer I think is what it's called but then I could have inside a singularity universe so like client connection or client socket and that would either be a generic... Either be a struck with a generic or better yet I think having having it be a macro might actually be better then
I think I cut out again, but the big idea was just to have either struct with the generic or of a macro for a universal client socket and a universal server socket. This would work as a rapper around the around like even the unpacking ideally, but especially the ID for responses. One of the things it could do would be if you call like socket, query, blah blah blah then that would actually run a loop and then every time it receives any packet it would first check if it's the correct type of response and if it is the correct response then great that's like the basic case. We just return the packet response packet directly, but if it isn't, then maybe we could have it in the the message queue or something and then and then every time we get something from the server that isn't what we're actively looking for then we just put it in the queue and then we keep listening until we get what we're looking for and then later on, we can pass on the the socket or whatever to maybe like the main loop and then in the main loop it would just listen to all of the events that happened and it would start from the message Q and then it would listen if there was anything else and yeah I'm pretty sure that's already implemented for for sockets but from what I know, I don't know any way of directly modifying and accessing the message to that however you sockets does it so I guess we would just have a system of having two cues but that's really fine. I don't really care about the performance right now for something so trivial as that and the other idea I had with the universal socket macro was that I could have it so that we list all of the all of the event types somehow and then it would generate from that its own like Eno for that and maybe it could be called like universal events or something like that and universal events isn't actually defined in singularity because universal events could change depending on the events that the clients and servers want to support so I'll just be inside a macro so that it could change and actually the good thing about having a macro would be that since we're having it we're gonna have to manually serialize and deserialize it so it's fine to actually not have the the type itself defined in a common place cause the universal client and server sockets are going to be macro so they're not actually gonna define anything but it's gonna be like a blueprint for how to define the types and that's fine because we're gonna be manually serializing in deer realizing
i'm starting to feel sleepy now, which is good. I should be sleeping right now but before I end my ramblings, I just had one more technical thing I wanted to say so I could get it out of my mind and that actually has to do with the implementation so I was planning for the universal rapper to be not run on a separate thread even though that actually wouldn't be a horrible idea I think just having universal rapper an actual I guess instance isn't really the word I'm looking for, but but like yeah, having an instance of a universal rapper just simply be an object and not be like in its own separate thread, and then you can have a bunch of connections to the universal rapper thread cause that kinda complicates everything again and also I feel like having a separate thread is gonna be a pretty significant cost in a way like I feel like if I was writing a client and I had to end I didn't even have the choice of not creating a separate thread then that would be a pretty big turn off for me, but I would have the actual universal rapper just be an object which stores maybe a message Q and then the actual connection to the UNIX socket itself and in terms of the rust borrow checker, I think for each connection there should only be one like one actual object of the universal rapper, but I think having it work with references will be would be useful and maybe I was thinking if you only have a reference to the universal rapper, the only thing you can do is call request and the reason why I thought good idea is it because I feel like the events itself should be centralized like it would be kind of weird if like a sub component of the client was listening to the event, but I don't know. I'm just saying this because I want to get everything in my head out of my mind to just clear any thoughts that might Lynge and get in the way of my sleep

2024/12/31 (next day)

Wow... that is a lot of yap that I am not going to read.

I think I remember most of it so its fine.
Just make a universal client/server socket wrapper handler (name in progress) macro.
I am thinking something like:

```rust
use singularity_standard::{KeyboardEvent, MouseEvent, WindowRequest}; // this could be merged as StandardEvent, but I don't actually know if that is better
use drag_crate::DragEvent;
use interapp_communication::IACRequest;

generate_universal_client_socket!{
    events: [KeyboardEvent, MouseEvent, DragEvent],
    requests: [WindowRequest, IACRequest],
    // this would somehow have information on the corresponding responses for each branch
    queries: [StandardQuery],
    // default would be Universal
    name_prefix: MyCool,
};
```

which would generate `MyCoolEvent`, `MyCoolRequest`, `MyCoolQuery`, `MyCoolResponse`, as well as `MyCoolClientSocket`.
The first four I just listed would just be enums.
The ClientSocket would have methods: `read_events`, `request`, and `query`.

I really like the python `*args` syntax, so I want something like:

```rust
generate_universal_client_socket!{
    // `*StandardEvent` would expand out to its enum branches, like `KeyboardEvent` and `MouseEvent`
    events: [*StandardEvent, DragEvent],
    requests: [WindowRequest, IACRequest],
    queries: [StandardQuery],
    name_prefix: MyCool,
};
```

I am not convinced that queries and responses are type safe yet.
Another syntax I was thinking about is:

```rust
queries: {
    StandardQuery: StandardResponse,
}
```

but that is even worse.
A relatively jank but safe way would be to just have a new method for each query, like: `query_window_size` and `query_clipboard`.
To avoid name collisions, I could eventually create a `StdQuerier` and `WhateverCrateNameQuerier` structs,
which are generated from a reference to the client socket and only have the ability to send queries under that crate.
So, you could call something like: `client_socket.std_querier().query_window_size()`.

The information for queries, requests, and their relations can be reduced to a set of named mappings from query data to response data.
Each element in the set can be represented like: Name: InputType -> OutputType.

...

I will now try to implement what I wrote inside of singularity macros.
By the way, I think I already mentioned this link, but I believe this is the most helpful proc macro resource: https://www.freecodecamp.org/news/procedural-macros-in-rust.

Trying to expand with `RUSTFLAGS='--cfg test' cargo expand --manifest-path singularity_common/Cargo.toml tests::macro_demo`
has a weird error, and it seems to be caused by the `RUSTFLAGS='--cfg test'` portion.

I got a little confused on the test targets, but it turns out, you are supposed to put the tests OUTSIDE of source, just like examples.
You should also put benches outside, and I didn't even realize benches were a thing.
This is the project layout guide: https://doc.rust-lang.org/cargo/guide/project-layout.html
Very useful, especially since I haven't been adhering to best organizational practices.

I remembered the thiserror crate, which I haven't used yet (I unfortunately don't do proper error handling).
This is an example from their [crates.io](https://crates.io/crates/thiserror):

```rust
#[derive(Error, Debug)]
pub enum MyError {
    Io(#[from] io::Error),
    Glob(#[from] globset::Error),
}
```

I think I can do something like this for events and requests (query and response are slightly harder).
I won't need the `#[from]` specification, because I would assume all event cases defined by the macro are already events themselves.
(if the client or server both made their own custom events, it would get messy even if they agreed)

By the way, if there are nested enums, (like MyEvent has ClipboardEvent has CopyEvent), then there would be multiple ids sent in a packet.
I could optimize later, maybe by comparing the lists of all supported id types on connection.

I am going to write a declarative macro for event combine, which was done manually in macro_demo's MyEvent.
I want the macro to look like:

```rust
combine_events!(MyEvent: [ClipboardEvent, DragEvent]);
```

...

actually implemented, and it looks like:

```rust
combine_events!(pub MyEvent => [ClipboardEvent, DragEvent], 9000);
```

the differences:
- I can add visibility qualifiers (an actual improvement)
- `:` to `=>`, just because declarative macros don't support `:`
- need to specify the event id
  - should talk about this

So, how should I procedurally assign the IDs?
The the current state of the macro requires the caller of the macro to generate the id themselves,
which is the safest way on my end, because if something goes wrong, then it is the user's fault.
But, I want to eventually automate this as well.
I will list ideas, and something to consider is how I am going to deal with versions:
- Just generate a random number non-deterministically
  - Would change at every compilation, I don't like this
- Generate a random number based on contents
  - Actually pretty good
  - Would change every version but is deterministic
  - Changing every version will preemptively catch possible inconsistencies, but will be finicky
- Generate based on the name
  - Would be same across versions, not sure if that is good
  - The pro case for this would be when an event adds more subevents
- Generate based on the name and the id's of the components
  - It would change id when the contents change, but not if the impls change (I thought this was the best idea, but now that I explained it, I think just the name might be the best)

For all the ideas, I should also include the name of the crate, so multiple crates could have distinct events with same names.

2024/1/1

Okay, I am on a new system with Fedora and Gnome (the one I've been working on and will continue to work mostly on was NixOS with KDE).
Luckily, getting it to run wasn't too bad, I just had to run `nix --extra-experimental-features nix-command --extra-experimental-features flakes develop`
because I don't know where my Nix global config file is and I can't permenantly enable nix commands and flakes.
And when it ran with the dependencies, it ran smoother than what I expected.
The display and mouse clicks worked, and the mouse clicks affected the display in the expected ways.
Unfortunately, the keyboard input just had no effect on the app.
For now, I will ignore this problem (sounds like something I will regret later),
because I just need to set up the infastructure for IPC, which doesn't need me to run until much later.

Anyways, the previous thing I did was make the `combine_events` macro.
Next up: combine requests.
After that, I think I can implement a 

Requests should be so similar to events (I'm pretty much just reimplementing rust enums),
that I have a very slight urge to create a meta macro that would be like:
`meta_combine!(Event, Request)` which would define the `Event` and `Request` traits,
and define the `combine_events` and `combine_requests` macro.
But, I won't do this.
Or, I could make everything under just the packet trait (not a bad idea, actually). 

Sidenote: I was trying to think of a better name than `combine`,
and remembered union types.
I looked it up, and it turns out rust actually does have a union keyword.
It seems kinda cringe though, ngl.
Regardless, I think `event_union` is a better name for the macro than `combine_events`.

...

It turns out I can't develop until I give smithay its dependencies,
because VSCode rust analyzer breaks right now.
I can run the dev flake in CLI, but I don't know how to enable it for all of vscode.
On my nix system, I used the nix env and direnv extensions,
but I can't enable flakes permenantly, so the automatic nix env breaks.
I might be able to get it working with [Docker](https://docs.docker.com/engine/install/fedora/).
VSCode has a [guide](https://code.visualstudio.com/docs/devcontainers/containers) on
dev containers with Docker.

That seemed like a hassle, so I just ran: `nix-env -iA nixpkgs.fontconfig`.

I actually hate nix now.
I tried to like it, but reproducible on any system is complete dog cheeks in practice.
I'm going to try to figure out how to do this without nix at all.

https://packages.fedoraproject.org/pkgs/rust-smithay-client-toolkit/ is a thing.
Running `sudo dnf install rust-smithay-client-toolkit` says it isn't a package though.

Okay, I just installed a bunch of dnf packages until it worked.
For future reference, some of them are:
- fontconfig
- fontconfig-devel
- libxkbcommon
- libxkbcommon-devel

...

I abstracted packets to not care about Event vs Request.
Queries and Responses might need to be slightly different though.

...

I don't think I need to use macros for universal client and server.

2025/1/2

At least for just events and requests, I can just do `UniversalClientSocket<Event, Request>` and same with server.

...

I think a derive macro would actually be better for making things into packets.
It is more flexible and would allow for other derives as well.

2025/1/3

I got communication between server and client on the same thread.
Next up (in no particular order):
- Set up testing for multiple processes
  - Proper testing? Yuck! I'm not going to do that until I absolutely need to
- Do Query and Response
  - Need a macro
- Set up the unix socket stuff inside of singularity_common
- Improve the current derive `PacketUnion` macro
- Implement into singularity
- Organize properly

I'm going to try setting up query and response.

I want to add modularity, so that its not all just flat query and response pairs.
I will try to come up with a basic example of usage (ignore implementation, because implementation is trivial with respect to usage):


```rust
let my_query_bundle: MyQueryBundle = todo!();
let addition_response: AdditionQuery::ResponseType = my_query_bundle.get_math_query_bundle().query_addition(socket, AdditionQuery(1, 2));
```

so from the object `my_query_bundle`, we would get the sub-bundle of type `MathQueryBundle`,
which would contain the query: `AdditionQuery`.

I think I have an idea from this.
The most basic type would be a `QueryPair`, which consists of a query type and a pair type.
Then, there is the `QueryBundle`, which is a collection of sub `QueryBundle`s and `QueryPair`s.
All `QueryPair`s have a default `QueryBundle` consisting of just that pair.

Something like: `universal_client_socket.query(AdditionQuery(1, 2))`,
would not reaveal the type of the response...
Actually, it could.

Holy guacamolie, I think I just talked myself into a brain blast!

```rust
pub trait UniversalQuerier {
    fn query<Q: UniversalQuery>(query: Q) -> Q::ResponseType;
}
impl UniversalQuerier for UniversalClientSocket {
    todo!()
}

// UniversalPacket is the one with the do and from data
// as well as the packet id
// It is currently called `PacketTrait`

pub trait UniversalQuery: UniversalPacket {
    type ResponseType: UniversalPacket;
}
```

I think I was previously overcomplicating this whole thing.
With this way, query and response should actually be easier than event and request.

On the server-side, I was thinking of handling responses in the same way
as I have done with the mpsc channels:
`client_handler.respond(move |query| { ... })`.

This approach is acceptable, but I think I need to just assume
that the server will not return the wrong query type.

With that, I will assume that the only packets are:
queries from client to server and responses from server to client.

Query raw data will contain:
- Packet length
  - already handled by the byte writer and reader
  - const size
- Packet type
  - To say that it is a query and not a request
  - (I could eliminate this by just saying everything is a query)
  - For all queries, this should be a constant value
- Query instance id
  - Unique to each instance of a query (eg, even if you query size multiple times, each query will have a different instance id)
  - const size like u64
- Query type id
  - This actually says what type of query the query is
  - This would distinguish between things like: QueryName vs QuerySize
  - const size like u64
- Query inner data (Optional)
  - This would be defined by the query type

It might make more hierarchical sense to put the query type id
before the instance id, but I think practically it makes more
sense to do instance id first, because instance id should be read
even if the query type id is unknown.
(it really doesn't matter, even though my reasoning is kind of bad)
Oh, another reason is that if I did have query bundles,
and stored query type hierarchically in the raw data,
then it would be better to have the instance id first
(even though storing hierarchy would be inefficient).

Response raw data will contain:
- Packet length
  - already handled by the byte writer and reader
  - const size
- Packet type
  - To say that it is a response
  - For all requests, this should be a constant value
- Prompt Query instance id
  - Would be the same as the query instance id that prompted this response
- Response type id
  - This should be easy to tell from the query, so I am considering just not having this
  - Will have a special id reserved for unknown queries
  - If the response has a wrong type id that isn't the null id, then panicing will be understandable
  - const size like u64
- Response inner data (Optional)
  - This would be defined by the response type

For unknown queries, the server should just give a response with
a special `Null` response type id (probably like 0).
I considered having a `NullResponse` packet type, a `Null` response type id,
no response type id and inner data on the null response,
or just having an additional boolean represent
whether the response is null or not.

I like this system (of flat pairs instead of tree organization) a lot,
that I might actually change the events and requests to this way.
I don't like it for the fact that it is structured in a flat way,
but it does seem to be the simplest implementation.
Actually, I don't like it that much (I am an indecisive person).
I think that sending should allow for flat sending to avoid boilerplate,
but recieving should allow for a hierarchical structure.
Maybe I am in denial because I can't figure out a way to get hierarchy
for queries.

In the end, I should stop caring the specific implementation.
I just want to make reasonable progress in a reasonable time.

Okay, I am going to start.

...

2025/1/10

I realize that the standard is actually little endian, not big endian.
I will change that later for everything.

I think I should also write a new manifesto for singularity,
since a lot of things have changed.
The summary will be that I am trying to abstract things like UI so that
everything can be organized by a central organizer and apps can work nicely
with each other.
Right now, there are different OS's which run apps and those apps have their
own ways of organizing their UI components.
With a browser, it is clearest to see how many layers of different protocols
there are.
This limits customizability on the user side
(eg: no standard way to set color pallete,
the closest thing to this is dark vs light which is just 2 options
and apps need to go out of their way to support it;
eg2: standard setting and shortcut management)
and prevents compatibility between apps.
HTML kind of does this, but it is bad.

...

There is something called `TypeId` in `std::any`,
which might allow me to do the id stuff by default.

...

Adding onto the philosophy, I want to be able to remove the sidebar and line counter from my text editor
very quickly, like doing inspect element and then save those settings into a view template.

2025/01/15

I have been working on other stuff, but today I worked on tests for the query response sandbox.
I should have committed before, most of the changes in this commit will be from a few days ago.
I have often worked on a pretty big change, almost gotten it finished, and then worked on other things for a few days before putting the finishing touch and committing later.
Ideally, I would be using github issues more often, but it seems unnecessary right now.
I will change my policy to just commit broken code at the end of the day if I know I will be working on other stuff for a few days.

Also, I looked into Unix Domain Sockets and TCP sockets, and in TCP, it seems like they know what response corresponds
to what request, because responses are ordered the same ways as requests.
I think I like the Uuid way more, even though it has slightly more overhead.

Erhm, I made thread_test to generate the minimal reproducible example, but it worked and then the simplified version of the original code started working too.
I will commit the two minimized versions before it doesn't work again.

...

NOOO, I committed, and ran without chaning ANYTHING.
It didn't work.
After further teseting, it seems to be non-deterministic because threads.

Okay, I fixed it (well, I ran it ten times in a row and it worked) and now I know why.
I had to do the listener creation before starting both threads, to ensure that the client wouldn't ask to connect on a socket before it even was created.
The fact that the error was non-deterministic actually led me to try this fix.

...

By the way, adding the `--show-output` flag to `cargo test` does show output, but only after the test ends.
I had to add `--nocapture` to debug why it doesn't even end.

...

I got the sandbox to work, so here are next steps:

- [x] Move unix domain socket helpers to `singularity_common`
- [x] Abstraction for bytes unwrapping/splitting
- [x] Make a new all packets sandbox, to merge the query response sandbox and the event and request code from sap
  - Probably should split the roles by packet type (eg, the clientside sap interface should have two different types of things for sending queries vs requests and two more different modules for parsing events vs responses). I guess this is pretty obvious actually.
- [x] Move the stuff in query response sandbox into sap
- [ ] Change all big endian to little endian

2025/01/16

It turns out query_response_sandbox sometimes non-deterministically fails,
but I feel like it will be fine.

2025/01/17

I have an abstraction idea that might be really bad,
so I won't implement it, but I wanted to write it down
for the sake of ...whatever.

I think I can further abstract event and response by just having
a general packet thing.

I think I am going slightly mad right now.
The difference is between defining a basic structure for packets and
providing the actual parsing, versus just letting them define the parsing stuff themselves.

...

I just realized that the server side connection doesn't even need a queue,
because I only needed queue because client might ask for a specific request.
I will fix this after committing though.

I think I figured out a way to jank the type system.
First, use the `type Query` instead of `<Query>`.
Then, I think I can make helper methods inside of the query responder,
which I believe will let me avoid problems with unknown size errors I would
have gotten if I put the helper functions there.
Additionally, I can make an InnerQueryRespond to prevent the helper functions
from being overridden and to prevent them from clogging the user's list of usable methods.

The actual calculations that need to be done when sending back a query response, given that we know it is a query:

1. Split `packet data` -> (`query instance id`, `query type id`, `query inner data`)
2. Try to match `query type id` to an actual `query type`
   1. In our case, we need to find the `query responder` with this `query type`
3. Generate `query object` of the correct `query type` from the `query inner data`
4. Generate the `response object` by asking the correct `query responder`
5. Generate `response inner data` from `response object`
6. Generate `response data` by combing `query instance id`, `response type id`, and `response inner data`

I think steps 2-5 should be in the trait helper functions,
because they require information about the type that I am not sure how to get from `&mut dyn QueryResponder<Query = dyn Any>`.

Uhh, the trait stuff is stricter than I thought.
You know what, that actually sounds like a problem for future me.

---

2025/01/18

Gosh, I wish I could take credit for this, but I asked chat gpt to help me debug or brainstorm a new way,
and it actually thought of a good solution.
It is like learning that CRANE (technically SALET, whatever) is the best worlde starter;
you don't want to use it because it is not your solution, but you kind of have to because it is the best.
The important section is this:

```rust
pub trait TypeErasedResponder {
    fn respond_erased(&mut self, query_data: &[u8], query_instance_id: QueryInstanceId) -> Option<Vec<u8>>;
    fn get_query_type_id(&self) -> IdType;
}

impl<Q, R, T> TypeErasedResponder for T
where
    T: QueryResponder<Query = Q>,
    Q: UniversalQuery<ResponseType = R> + TryFromData + 'static,
    R: PacketTrait + 'static,
{
    ...
}
```

and I swear I only looked at the type hinting part of its code, because I don't want to just use generative AI
for singularity.
I just used it to get an idea and I will write the code myself.

But dang, I really don't know how to feel about all this AI stuff.
Like, I saw a video about a writer who was replaced by AI, and now I am just wondering if that is going to happen to me.
That, in addition to the oversaturation of CS people in general is quite frightening.

At least I can look at the current state of Devin and know that AI won't outperform humans yet,
but it will only grow smarter.

I'm not sure how the ethics of all this works either, because I suspect that Chat GPT was trained on lots of data
without permission, and I kind of feel like I am stealing from those people, but my current position on the matter
is that consulting Chat GPT is somewhere between a rubber duck on steroids and just looking at someone else's code.
I believe that pretending like generated code is someone else's code is a pretty safe view (less likely to be
intellectual theft), so I will just ask it for ideas and help debugging.

To my credit, I was already trying to do something similar to this in the `InnerQueryResponder`.
The big difference is that I just had to take in the argument of type `Vec<&mut dyn InnerQueryResponder>` to begin with.
I actually feel like I could have thought of this if I gave myself a few more days,
but I didn't and I was able to save those hours so I can make more progress, so I can't complain.

...

I will actually implement the rest of the logic for `handle_incoming` now.

TODO:
I am getting an annoying clippy warning about `InnerQueryResponder` being more private
than the `handle_incoming` function,
and I still want to keep it private to prevent tampering,
but I will consider fixing it by having yet another trait, if that is possible.

...

I couldn't do that easily, so I just made it public with a sign that asked the user not to override it.

---

2025/01/19

Holy guacamole, I might be the GOAT of all time of all time.

First time running, and no errors!

Put in that meme of kronk going "Yeah, its all coming together"
because it all really did just come together.

...

Okay, so despite that MASSIVE W, I still need to add more testing,
which I didn't do because I was scared it wouldn't work.

I think the single process tests work, I will commit this then test on different processes,
then put the `all_packets_sandbox` into `singularity_common`.

...

Today might be my day, multi process worked smoothly as well.
I will commit then move it to the actual libraries.

...

The little warnings were driving me crazy, so I got rid of them.

---

2025/01/19

There are still things to improve with `sap`, but I think I can start incorporating it
into singularity now.

I think I should give a high level overview of the sub-projects for singularity:

- Singularity Application Protocol (SAP)
  - It describes a way to send extensible packets between any sap supporting server and a sap supporting client
  - Sap is like the wayland protocol, the sap server would be your desktop environment, and the client would be any wayland app
  - For now, each SAP connection corresponds to exactly one window
- Singularity Project Manager
  - A way of organizing projects so that projects can talk to each other
  - This will be used by the singularity tab manager
  - Primary purpose is to neatly organize how persistent data related to singularity is stored
- Singularity Tab Manager
  - Possible name: Stabor (singularity tab organizer)
  - This is the official SAP server, the only one that I will be working on (probably)
  - Its job is to manage and provide tools for sap clients (tabs)
    - Handle compositing and UI
    - Relay communication between tabs
    - Organize tabs
- Tabs
  - Each tab is a SAP client
  - Example Tabs:
    - Chro
      - The time manager
      - I might make this a seperate thing, and have it support terminal or SAP Gui
    - Terminal (Sterm ?)
      - (realistically, once I have terminal, I unlock most apps I need)

I will start deprecating the old methods of communication and integrating sap into the singularity tab manager now.

As expected, there is a lot of things I need to rethink.

2025/01/23

Okay, I didn't commit like I planned, and you can tell from the log dates that its been a few days.

Well, I guess I will think of everything before committing since I am late anyways.

I will split all my modules into these crates:

- Singularity Common/Utils
  - Datastructures and stuff that are used by many things
- Singularity Macros
  - Should be in `singularity_common`; this would ideally just be a module in common but proc macros currently need their own crates
- Singularity UI
  - Abstracts all the Backend specific stuff to provide the bare minimum UI support
  - Currently just supports wayland
  - No change needed
- Singularity Project Organizer
  - (SPORG?)
  - Not sure about this name
  - Used to be in `singularity_common`
- Singularity SAP
  - Yes, the name is redundant, whatever
  - Used to be in `singularity_common`
- Singularity STABOR/SDE
  - Alternative name: Singularity Desktop Environment
  - This is the canonical implementation of the SAP server to handle tabs
  - This used to be just `singularity_manager`
- All the tabs can be in their own crates or something

Something I want is extensible UI widgets with shared libraries,
and this idea can be extended to things outside of UI.
There are UI primitives. For the sake of example,
lets just say that the only UI primitive is the pixel grid.
The set of all primitives is already agreed upon,
and must be standardized.
But, suppose an app wanted to display text.
Without widgets, the app would have to draw the text
itself onto a pixel grid.
This is bad for a few reasons.
First, this lacks standardization.
If there were multiple apps, that had to do it themselves,
then all of them would have different fonts and it would be ugly.
Secondly, due to the lack of standardization,
user side customization would be difficult.
Also, this could add performance overhead.

The solution would be to have shared widgets.
Each tab can return some composite of widget and primitives.
If some widget protocol is manually implemented by the user's
display environment, then that implementation is used.
But, when a tab uses a widget protocol, it must also define
some shared library type thing that would handle the default
case.

...

I think I might just rewrite most of sporg and the tabs.

2025/01/30 12:52AM

(I am writing this entry to talk about talking to someone about singularity.
The other changes are things I've been working on and is unrelated to this.
Ik, I said I will commit more but too late.)

I talked to someone who has a lot of experience.
Other than the one friend I talk to (@glolichen), this person is
kind of the first person I've talked to about singularity in a pretty deep level.
They didn't care about the actual code, but they asked a lot about the idea itself.
We talked over email for a few days, and today (technically 2025/01/29 7-8PM),
we talked over phone.

I really have to sleep so I'll keep this short.
If it really matters, I will put in the email logs later, so I will just summarize the call.

The main question/advice was to just make an actual window manager.
The pros are performance, simplicity, and the fact that I can simply use any app that already exists for wayland (or x11 if I make a x11 wm).
But, the inter-app stuff wouldn't be a part of the wm.
They mentioned `ocl` for the office suite.
They also said that Xserver was a program on its own, and I should experiment by running bare xserver from terminal then running apps manually from the cli as well,
like xclock.

I suggested, for the inter app stuff, if I were to make a window manager, I could have a special app to handle all
the inter app stuff.
But, I didn't really explain my use-case fully.

I am not completely sold on the idea of making a window manager yet, for a few reasons.
The first is the inter-app stuf.
Secondly, they said making a text editor and terminal from scratch is impossible for just one person.
But, I think I can embed some other open source terminal for singularity at the worse case and run vim on it
(but at that point, I could have just made a terminal session manager).
I will do research on that though:
[this reddit thread](https://www.reddit.com/r/rust/comments/1d47bl1/suggestions_on_a_gui_framework_for_embedding_a/)
says I can embed alacritty.
It suggests using [alacritty embeds](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/#reexports).
It gives [this example](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/ui/bin/terminal/).

I will have to think about this for a while.

2025/01/30 1:35AM

Roadmap to recovering from restructuring:
- [x] Implement the TODO's in singularity macros
  - [x] Test by making a print_test_server in sde and print_testor in standard tabs as a bin, where standard packets are sent and printed on both ends
- [x] Add basic `standard_packets`
  - [x] TODO: bare minimimum packets for standard packets
- [ ] Figure out how to do the cfg feature stuff (might already be working, if so, just verify it is working)
- [ ] Start the actual sde, that just displays the UI, no organization
- [ ] Implement some basic thing in std tabs, like the worst cookie clicker ever
- [ ] Add organization code back into it
- Figure out the rest (eg: adding sporg into it)

2025/01/30 7:34 PM

I posted a [question on macro expansion](https://users.rust-lang.org/t/expanding-inner-macros/124887) to the rust lang forum
because I realized I needed to learn to ask for help.
There is a chance that no one answers, but that is fine.
I had to create a simpler version of my question, but if that simpler version is answered,
I should be able to jjust apply that solution to my actual macros.
I will just commit now to log this.

2025/02/01

I got two responses so far.
Feels unusual that Steffahn replied, because I've seen their replies on a lot of
the rust threads in the past, and I kind of assumed they were a celebrity or
something.
But it does make sense that the people who comment the most would be the ones to
comment on mine.

The responses said that it isn't really possible.
But, there is a [nightly feature](https://users.rust-lang.org/t/expanding-inner-macros/124887/5)
which should hopefully make it possible eventually.

---

I am adding the basic `standard_packets`, and I am not sure how to do `SpawnChildTab` because
I have to represent the idea of a generic tab.
This is definitely an important feature, but I don't think it should be a standard feature,
so I will just have it not be one.
A related feature would be `SpawnDefaultEditor`, `SpawnDefaultBrowser`, and etc
(or I could have `DefaultEditorQuery`, and etc).
Well, now that tabs are processes, I could represent a generic tab with the path to run the process
along with the arguments, much like `Command`.

Also, I think the display should be done with shared memory but the problem is that I don't know how to do that in rust.

2025-02-11

I felt pretty stuck (both in this project and in life, ha ha ha ha...),
but I am working on changing how the derive macros work.
There are now 3 different derive macros:
- `Packet`
  - Impl's `PacketTrait`, but assumes `Datable` is implemented elsewhere
  - Mainly just generates the `PACKET_TYPE_ID`
- `PacketUnion`
  - To be used on an enum where each variant corresponds to a unique `PacketTrait`
  - Impl's `Datable` (`ToData` and `TryFromData`)
- `Datable`
  - Impl's `Datable` for an enum or a struct composed of `Datable`'s

The change that isn't obvious but should be mentioned is Datable impl for enums.
The previous impl is now packet union, and used the packet type id to differentiate
between variants.
The new impl for this is to define the numbers corresponding to variants in the macro.

---

I implemented Datable for a bunch of classic types, and also added proper unit tests
for them.

I am also implementing Datable for `singularity_ui` elements,
and I've just been copy and pasting the types into another file that can access the Datable macro
and singularity_sap, then using the derive, then expanding,
copying the expanded output, and pasting it into byte_stream.
I am doing all this because rust doesn't allow circular imports easily,
and I also can only use derive macros at teh struct/enum definition.
Maybe there is a way to automate this in `build.rs`,
but for this scale, that would be more work.

---

I implemented this way recursively, until `UIElement`.
I don't have the UI Events yet, but I can now get started with some very basic communication.

I was going to make a boring tab called `UIElementDemo`,
but life is too short to do boring stuff.
I am going to make a fortune teller tab,
which is functionally just a wallpaper, but I want it to be something fun.
It is going to be like one of those quirky `.bashrc` settings.
My friend pipes fortune into cowsay, and I think that is cool.

I'll just have the fortune teller display the time and a fortune.

...

I have to think about how to represent tabs in singularity.

2025/02/15

I tried to get either nvim or emacs working,
and I really want to get into emacs,
but it has hour-long videos of just the basic config,
and when I combine that with the fact that I use NixOS,
I think I have a better chance of just finishing singularity
than setting up Emacs.
People joke about not being able to exit vim,
but I can't even set up Emacs.
(I know Doom emacs and spacemacs is a thing,
but they work even less with NixOS.)

Anyways, let me get back to singularity.
The problem from before was to do with representing tabs.

I will outsource the hard part of this decision to my future self,
and for now, for the most basic purposes, I just need it to be
a connection to the client.
I will put this inside `singularity_sde`,
even though this should belong in a singularity server toolkit.

I think I can just reuse what I deleted in the restructuring,
and the most up-to-date stuff for that is in:
https://github.com/mathkimchi/singularity/tree/8e8e02348cb41f0d077783aaa2ca7d3d69288d22/singularity_common/src/tab.

The hard part will be to somehow allow representing this in data
for the following features:

- Preserving tabs in between sessions
- Spawning Children Tabs

Now, the obvious way would be to store the commands that
spawn the processes that spawn these.
But, this idea on its own doesn't address the problem of
matching tabs to actual processes.

An example scenario of this would be:
When the SDE spawns a process and expects the process
to request a tab and the SDE wants to put that tab
somewhere in the hierarchy.
This is the case for both restoring the tabs from a past session
(want to put each tab in the past session's hierarchy),
and for spawning a child tab
(want to put the child tab as a hierarchical child of parent).

Also, it would be difficult to even get this for new tabs,
so it is hard to even start.

I can think of more than one way of resolving this:
Firstly, just have the tabs give this information
to the SDE.
This could be like a normal request, or as a special request
that is sent on connection start.
Now I will explain the radical solution:
Alternatively, rest the "burden of initiative"
on the server.

I explained it in the car with a voice recording,
and I am too lazy to transcribe it all.
I won't clutter the repo with the sound files,
but on my phone, it is saved as:
`Hobey Backer Memorial Ice Rink`,
`Princeton University 63`,
and `Princeton University 81`.
(I am not sure why it skipped from 63 to 81.
The phone was weird and kept stopping,
maybe because the audio was connected to the car.)

But, I am not sure about this idea.
If there was a "singularity way" of doing things,
then I think this idea would be 100% the singularity way.
I can't explain why, but just this non-conforming,
complicated, unnecessary challange of standard practice is precisely
the type of thing an idiot like me would enjoy.
Plus, "burden of initiative" is a cool term,
so I guess I just forced myself into doing this.

In all seriousness, I think a change of this magnitude,
at this stage (when changes and commits are already slow),
requires careful consideration.

I am reminded of local web services like openwebui.
But, to my knowledge, services mean that they run in the background,
and I don't want to do that.

I think I could have something similar that caters better to my wants
with shared libraries.

2025-02-17

I don't think I will be doing the webpage-like way.
but I still want to support many of the features like
the redirect pages system.
but I think the local storage should suffice for reopening tabs.

I think piping stdio could actually be a not bad solution,
and looking back at a [comparison of ipc methods](https://3tilley.github.io/posts/simple-ipc-ping-pong/#approach-1-pipes),
I realized piping is actually faster than TCP/UDP sockets,
and [a similar article](https://www.baeldung.com/linux/ipc-performance-comparison)
shows compares pipes with Unix socket specifically, and says it is 30% faster
(this much of a difference doesn't really matter, but knowing that a new approach
is a speed upgrade makes me feel good about implementing it).
Also, I think the piping is similar enough to unix sockets such that it wouldn't
be difficult to support both.
Piping will allow for SDE side initialization, but not the more webpagey features
like redirecting.

2025/02/18

Okay, so, I decided I will implement just Unix Sockets for now,
but in serialization, represent tabs as commands.

...

Actually, I am trying to fix the code I previously wrote,
and a lot of the past code expects the tab to be initialized by the sde.

2025/02/19

Stdin doesn't have a vanilla way of polling,
so I made a wrapper for `Read` using mpsc.

The downside of the piping is that my main tool for debugging just disappeared.
Ways to debug would be to create a request, or to write to files.

I am going to change the old idea of `tab_type` to `tab_command`.

2025/02/21

I implemented datable for the UI Event stuff,
so now I think I am almost done with migrating to multiprocess.

As a reference, this is from 2025/01/30 (has been updated as I went)
Roadmap to recovering from restructuring:
- [x] Implement the TODO's in singularity macros
  - [x] Test by making a print_test_server in sde and print_testor in standard tabs as a bin, where standard packets are sent and printed on both ends
- [x] Add basic `standard_packets`
  - [x] TODO: bare minimimum packets for standard packets
- ~~[ ] Figure out how to do the cfg feature stuff (might already be working, if so, just verify it is working)~~
- [x] Start the actual sde, that just displays the UI, no organization
- [x] Implement some basic thing in std tabs, like the worst cookie clicker ever
- ~~[ ] Add organization code back into it~~
- Figure out the rest (eg: adding sporg into it)

I don't know what I meant by the cfg stuff,
I assume it is working.

I ended up never having to remove the organization code,
because I was able to reuse 90% of my old SDE.

I know what I still need to do, but right now, I think
I will commit this, and merge this branch into dev,
marking [#3](https://github.com/mathkimchi/singularity/issues/3) as completed!
(I should have made smaller issues, but whatever)

## [#6](https://github.com/mathkimchi/singularity/issues/6)

I made a new issue (for future reference, clicking the create branch with GitHub desktop is probably better than local).
You can see it in the second heading title for this section.
From now on, I think this will be the formatting of my devlogs.

I am going to strive away from the object orientedy way I previously handled this,
meaning the structs will have data but the bulk of the logic will be outside the struct.
Actually, I'll put the file manager specific stuff along with the struct (as an impl),
and have the more tab-related boilerplate stuff (client stream) outside.

This actually requires session data, so I will need to think about that as well.
I will start off by using a hard coded path.

...

I got displaying to work, but suddenly, none of the keyboard inputs are working,
even for shortcuts outside the tab like `Ctrl+Q` and tab traversing.
The old commit isn't working either.

Okay, it was because NUMLOCK was interfering with the shortcuts.
I really gotta do something about shortcuts.
You know what, I am going to make that a new issue.

...

2025/02/22

I am implementing Editor, which previously used the components.
I will add just the textbox component, but I am getting import conflicts by having it in singularity common,
so I will move this into a new rust package: `singularity_sttk` (singularity singularity tab toolkit)
which should be like the Smithay client toolkit.

...

Allowing file manager to spawn editor went pretty smoothly.
I think I can commit and PR this.

## [#11](https://github.com/mathkimchi/singularity/issues/11)

2025/03/03 2:30 AM

I am working on the big file for the SDE (the `singularity_sde/src/project_manager/mod.rs`),
and autoformat simply does not work on this thing.
I think it is caused by the large size of the file.
Handle input is the immediate biggest suspect, since it is long and has a lot of nests with all the shortcut cases.
Like I previously mentioned, I want some standardized method of doing shortcuts.
After this commit, I will actually start with that.

2025/03/03

I will make UI actions (more or less shortcuts).
I initially wanted to somehow make some UI actions impossible to represent from some modes,
but I think it is fine to have that.

Mode is just FSM, and UI actions are like events/events.
A [stack overflow post](https://stackoverflow.com/questions/35439546/a-pattern-for-finite-game-state-machine-in-rust-with-changing-behavior)
has a pretty good example, and it allows for incompatable events to be represented, which I guess

This [video](https://www.youtube.com/watch?v=KdLTqyblbo4) seems to use a similar example,
so I guess the pushing machine is a famous example or something.

In this example, I would want it so that the user wasn't even allowed to push on lock and coin on unlock
but it seems that they make it possible and just ignore.

I feel like I should make this a new github issue, but whatever.

...

Implemented the actions, crossing my fingers before running.

Oh yeah, it almost all worked first try.
I just had to change command palette from alt+shift+P to ctrl+shift+P.

...

Many improvements to be made for actions:
- Define actions in a seperate file to be read at runtime
  - Special syntax for shortcuts during specific mode, for shortcuts regardless of mode, special group of keys for tree traversal operations, etc, kind of like regex
- Somehow make UI actions more safe
  - Ex: If a ui action only happens in Mode::ChoosingFocus, then the UI Action can actually contain the choosing focus' data
- Have extensions define their own actions too

...

Okay, I don't want to dwell on actions, but I did make things slightly better by adding a common shortcut style case.
I didn't vigorously test the new change like I did with the Actions first time, but nothing seems to be broken.

...

2025/03/04 12:56AM

One more change before sleep: add display for command palette.

Oh man, I have a 8AM final tomorrow that I should have studied for...

But for good news, I am almost at the 10k lines of rust anniversary for Singularity!
(I swear I don't just constantly look at the word count; I do it less than one day per week.)

This is the total rust `wc.sh`:
- 9114 lines
- 26513 words
- 322802 characters

Counting the DEVLOG seperately, it is (*was, before this line):
- 2740 lines
- 24155 words
- 141563 characters

Progress today has been very fast.

Okay I'm going to sleep now.

---

2025/03/04

I have an idea for making invalid actions (transitions) unrepresentable.
https://hoverbear.org/blog/rust-state-machine-pattern/
goes over some methods, but I want to try something slightly different.
There are some insights from that article that I will attempt in my approach as well:

- Action object consumes state
- State variants get their own structs
  - Doesn't impact safety but improves clarity imo

I wanted to do something like:

```rust
let action = Action::from_ui_event(self.mode, ui_event);

self.mode = Self::next_state(action);
```

But even if I promise to put the value back, the [borrow checker doesn't let me](https://users.rust-lang.org/t/can-i-move-out-of-a-mutable-reference-as-long-as-i-promise-to-put-some-valid-data-back-in-before-i-use-it-again/108417/3).
[This crate](https://github.com/alecmocatta/replace_with) seems to take care of that,
but I don't want to use it.

I really don't like doing this, but I will swap the mode with a default, the `Mode::TabFocus`
and override it afterwards.

...

hmmm...
I don't like this at all.
I did things like:

```rust
Quit(Mode),
ChooseFocus(ChoosingFocusMode),
```

but I don't like how it actually turned out.
I'm actually just going to copy the current `mode.rs` into here:

```rust
use crate::tab::TabHandler;
use singularity_common::utils::tree::{
    id_tree::IdTree,
    tree_node_path::{TreeNodePath, TreeTraverseOperation, TREE_TRAVERSE_KEYS},
};
use singularity_ui::{
    display_units::DisplayArea,
    ui_event::{Key, KeyModifiers, KeyTrait, UIEvent},
};

// #[derive(Debug, Clone, Copy, Default)]
#[derive(Debug, Clone)]
pub struct TabFocusMode;

/// If ChoosingFocus, there should be a special window app focuser
#[derive(Debug, Clone)]
pub struct ChoosingFocusMode {
    focusing_index: TreeNodePath,
    plucked: Option<IdTree<TabHandler>>,
}

#[derive(Debug, Clone)]
pub struct CommandPaletteMode {
    /// The string that the user has typed so far.
    /// TODO: make this use Textbox component to support cursor and stuff without duplicate code
    command_buffer: String,
}

#[derive(Debug, Clone)]
pub enum Mode {
    /// Focused on some app
    TabFocus(TabFocusMode),
    /// If ChoosingFocus, there should be a special window app focuser
    ChoosingFocus(ChoosingFocusMode),
    CommandPalette(CommandPaletteMode),
}
impl Mode {
    // pub fn try_as_choosing_focus(&self) -> Option<(&TreeNodePath, &Option<IdTree<TabHandler>>)> {
    //     match self {
    //         Mode::ChoosingFocus {
    //             focusing_index,
    //             plucked,
    //         } => Some((focusing_index, plucked)),
    //         _ => None,
    //     }
    // }

    pub fn try_as_choosing_focus_mut(
        &mut self,
    ) -> Option<(&mut TreeNodePath, &mut Option<IdTree<TabHandler>>)> {
        match self {
            Mode::ChoosingFocus(ChoosingFocusMode {
                focusing_index,
                plucked,
            }) => Some((focusing_index, plucked)),
            _ => None,
        }
    }

    pub fn try_get_focusing_index(&self) -> Option<&TreeNodePath> {
        match self {
            Mode::ChoosingFocus(ChoosingFocusMode {
                focusing_index,
                plucked: _,
            }) => Some(focusing_index),
            _ => None,
        }
    }
}

/// Look at devlog 2025/03/03
pub enum UserAction {
    // SECTION - closing

    //
    /// Ctrl+q quits
    Quit(Mode),
    /// Ctrl+Shift+W recursively closes focused tab and children
    RecursivelyCloseFocusedTab(Mode),

    // SECTION - tab hierarchy operations

    //
    /// Alt+Enter from ChoosingFocus mode
    /// REVIEW: Rename to select?
    ChooseFocus(ChoosingFocusMode),
    /// Alt+Enter from NOT ChoosingFocus
    OpenFocusChooser(Mode),
    /// Alt+TreeTraverseKey should be like alt tab for Windows and Linux but tree based
    TraverseTabTree(Mode, TreeTraverseOperation),
    /// Alt+Windows+TreeTraverseKey swaps position of focused and what would be the new focused
    TreeSwapTraverse(Mode, TreeTraverseOperation),
    /// Alt+Windows+P
    /// I am fine with this technically being two different things to do but one action
    /// TODO: split this
    PluckPlace(Mode),
    /// Alt+Windows+Enter swaps actually focused and focusing
    TreeSwap(ChoosingFocusMode),

    // SECTION - command palette

    //
    /// Ctrl+Shift+P opens command palette (see: https://github.com/mathkimchi/singularity/issues/11)
    OpenCommandPalette(Mode),
    /// ESC quits command palette (see: https://github.com/mathkimchi/singularity/issues/11)
    QuitCommandPalette(CommandPaletteMode),

    // SECTION - tiling operations

    //
    // /// Alt+ArrowUp maximizes focused tab
    // MaximizeFocused,
    // /// Alt+ArrowDown minimizes focused tab
    // MinimizeFocused,
    // /// Logo+"=" (represents "+") increments title split
    // IncrementTileSplit,
    /// Logo+t transposes selected tile's container (hor<=>vertical)
    TransposeTileParent(TabFocusMode),
    /// Logo+s swaps selected tile's siblings
    SwapTileSiblings(TabFocusMode),

    // SECTION - misc

    //
    /// Key press isn't any of the keyboard actions; forward it to focused
    ForwardKeyPressTab(TabFocusMode, Key, KeyModifiers),
    /// Key press isn't any of the keyboard actions; forward it to command palette
    ForwardKeyPressCommandPalette(CommandPaletteMode, Key, KeyModifiers),
    /// currently, the only None case is when non-shortcut is performed on tab choosing mode
    /// REVIEW: is this good? just use option?
    NoAction(Mode),
    /// Resized. Currently ignore.
    WindowResized(Mode),
    /// Mouse press
    MousePress(Mode, [[u32; 2]; 2]),
}
impl UserAction {
    /// This is really to get around lack of if let in match
    ///
    /// For shortcut-like commands.
    fn handle_char_key_shortcut_presses(
        curr_mode: Mode,
        key_char: char,
        key_mods: KeyModifiers,
    ) -> Result<Self, Mode> {
        Ok(match (curr_mode, key_char, key_mods) {
            // Ctrl+Q
            (curr_mode, 'q', KeyModifiers::CTRL) => Self::Quit(curr_mode),
            (
                Mode::TabFocus(TabFocusMode) | Mode::ChoosingFocus { .. },
                'w',
                KeyModifiers::CTRL,
            ) => Self::RecursivelyCloseFocusedTab(curr_mode),

            // Alt+Enter from ChoosingFocus mode
            (Mode::ChoosingFocus(mode), '\n', KeyModifiers::ALT) => Self::ChooseFocus(mode),
            // Alt+Enter from NOT ChoosingFocus
            // NOTE: this pattern must be behind choosing focus
            (_, '\n', KeyModifiers::ALT) => Self::OpenFocusChooser(curr_mode),
            // Alt+TreeTraverseKey
            (_, key_char, KeyModifiers::ALT) if TREE_TRAVERSE_KEYS.contains(&key_char) => {
                // `' '` is a placeholder for some key that isn't in tree traverse
                // sad that match doesn't support if let syntax
                Self::TraverseTabTree(
                    curr_mode,
                    TreeTraverseOperation::from_char(key_char).unwrap(),
                )
            }
            // Alt+Windows+TreeTraverseKey swaps position of focused and what would be the new focused
            (
                Mode::TabFocus(TabFocusMode) | Mode::ChoosingFocus { .. },
                key_char,
                KeyModifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                    caps_lock: false,
                    logo: true,
                    num_lock: _,
                },
            ) if TREE_TRAVERSE_KEYS.contains(&key_char) => {
                // sad that match doesn't support if let syntax
                Self::TreeSwapTraverse(
                    curr_mode,
                    TreeTraverseOperation::from_char(key_char).unwrap(),
                )
            }
            // Alt+Windows+P
            // I am fine with this technically being two different things to do but one action
            (
                Mode::TabFocus(TabFocusMode) | Mode::ChoosingFocus { .. },
                'p',
                KeyModifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                    caps_lock: false,
                    logo: true,
                    num_lock: _,
                },
            ) => Self::PluckPlace(curr_mode),
            // Alt+Windows+Enter swaps actually focused and focusing
            (
                Mode::ChoosingFocus(choosing_focus_mode),
                '\n',
                KeyModifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                    caps_lock: false,
                    logo: true,
                    num_lock: _,
                },
            ) => Self::TreeSwap(choosing_focus_mode),

            // Ctrl+Shift+P opens command palette (see: https://github.com/mathkimchi/singularity/issues/11)
            (
                Mode::TabFocus(TabFocusMode) | Mode::ChoosingFocus { .. },
                'P',
                KeyModifiers {
                    ctrl: true,
                    alt: false,
                    shift: true,
                    caps_lock: false,
                    logo: false,
                    num_lock: _,
                },
            ) => Self::OpenCommandPalette(curr_mode),
            // ESC quits command palette (see: https://github.com/mathkimchi/singularity/issues/11)
            // '\u{1b}' seems to be the char for ESCAPE
            (Mode::CommandPalette(command_palette_mode), '\u{1b}', KeyModifiers::NONE) => {
                Self::QuitCommandPalette(command_palette_mode)
            }

            // Logo+t transposes selected tile's container (hor<=>vertical)
            (
                Mode::TabFocus(tab_focus_mode),
                't',
                KeyModifiers {
                    ctrl: false,
                    alt: false,
                    shift: false,
                    caps_lock: false,
                    logo: true,
                    num_lock: _,
                },
            ) => Self::TransposeTileParent(tab_focus_mode),
            // Logo+s swaps selected tile's siblings
            (
                Mode::TabFocus(tab_focus_mode),
                's',
                KeyModifiers {
                    ctrl: false,
                    alt: false,
                    shift: false,
                    caps_lock: false,
                    logo: true,
                    num_lock: _,
                },
            ) => Self::SwapTileSiblings(tab_focus_mode),
            _ => {
                return Err(curr_mode);
            }
        })
    }

    pub fn from_ui_event(mut curr_mode: Mode, ui_event: UIEvent) -> Self {
        if let UIEvent::KeyPress(key_event, key_mods) = &ui_event {
            if let Some(key_char) = key_event.to_char() {
                match Self::handle_char_key_shortcut_presses(curr_mode, key_char, *key_mods) {
                    Ok(shortcut_action) => return shortcut_action,
                    Err(returned_curr_mode) => {
                        curr_mode = returned_curr_mode;
                    }
                }
            }
        }

        match (curr_mode, ui_event) {
            // Key press isn't any of the keyboard actions; forward it to focused
            (Mode::TabFocus(tab_focus_mode), UIEvent::KeyPress(key, modifiers)) => {
                Self::ForwardKeyPressTab(tab_focus_mode, key, modifiers)
            }
            // Key press isn't any of the keyboard actions; forward it to command palette
            (Mode::CommandPalette(command_palette_mode), UIEvent::KeyPress(key, modifiers)) => {
                Self::ForwardKeyPressCommandPalette(command_palette_mode, key, modifiers)
            }
            // currently, the only None case is when non-shortcut is performed on tab choosing mode
            (Mode::ChoosingFocus { .. }, UIEvent::KeyPress(..)) => Self::NoAction(curr_mode),
            // Resized. Currently ignore.
            (_, UIEvent::WindowResized(_)) => Self::WindowResized(curr_mode),
            // Mouse press
            (_, UIEvent::MousePress(location, container)) => {
                assert_eq!(container, DisplayArea::FULL);
                Self::MousePress(curr_mode, location)
            }
        }
    }
}
```

Now I am going to revert the changes and commit just the devlog.

I got a very basic spawner in command palette to work, but I am not particularly proud of my implementation.
I will mark [#11](https://github.com/mathkimchi/singularity/issues/11) as closed for now,
but there are things in the github discussion that I want to revisit later.

Also, I am probably going to take a few breaks because I have to study for physics and do academic coding projects.

## [#16](https://github.com/mathkimchi/singularity/issues/16)

Did a bit of brainstorming in the issues.

2025/03/13

I will put the plugin api inside `singularity_sap`,
since this is similar.
I am looking at Zellij's [`ZellijPlugin` trait](https://docs.rs/zellij-tile/latest/zellij_tile/trait.ZellijPlugin.html) and Zed's [`extension_api`](https://github.com/zed-industries/zed/tree/main/crates/extension_api) for inspiration.

This is actually kind of similar to my [old implementation of tabs](https://github.com/mathkimchi/singularity/blob/19d6deb7ee7612ab096ded41a324e4e41da6e508/singularity_common/src/tab/mod.rs).

For the wasm in rust itself, these are resources:
- https://benw.is/posts/plugins-with-rust-and-wasi
- https://blog.wasmer.io/executing-webassembly-in-your-rust-application-d5cd32e8ce46

2025/03/14

Okay, the [new thread entry](https://github.com/mathkimchi/singularity/issues/16#issuecomment-2725875195) says all that needs to be said.

I will commit the WASM attempt now and revert everything except for this devlog.

2025/03/24

I have been kind of burnt out, but I want to work on improving the type system for the packets, being the events, requests, queries, and responses.
Since I will add query-response for between plugins, I might add a timeout duration for query.

Brainstorm usage:

```rust
pub struct ShortcutEvent {
    key_char: char,
    command_key: bool,
}

packet_union! {
    name: MyEventUnion,
    // packet: Event,
    packets: [
        ShortcutEvent,
        // python kwargs syntax
        **StandardEventUnion,
        **OtherEventUnion,
    ],
}
```

For additional safety, I might also want to additionally be able to specify: `MyEventUnion: PacketUnion<EventPacketType>`.

2025/03/26

The first thing I want to do is to differentiate the PacketUnion as its own trait.
Previously, I was using the Datable's to_data and try_from_data to convert between packet union object (eg `MyEvents`) and data,
but I will give packet union trait seperate functions to avoid confusion.
Additionally, if possible, I will start by only changing events, and leaving requests alone (query and response were always a bit different).

I'm not sure if I want to make a derive proc macro (current before changes) or declarative macro (tried a long time ago, but didn't like that I could use further macros).

The problem with blind recursive where packetunions themselves just act like packets with their own packet ids is that
because we don't know the depth and what is a fundamental packet vs packet union,
if there is a packet union with a packet union inside of it, then we will match the packet type id to the packet union instead of each of the packets within the inner packet union.
My brain is kinda fried today, does that make sense?

When recieving a packet, use the generic of packet union, when sending a packet, use the packet type.

Lines 1968-2011 of the DEVLOG define the old structure of query and response packets in data form:

```markdown
Query raw data will contain:
- Packet length
  - already handled by the byte writer and reader
  - const size
- Packet type
  - To say that it is a query and not a request
  - (I could eliminate this by just saying everything is a query)
  - For all queries, this should be a constant value
- Query instance id
  - Unique to each instance of a query (eg, even if you query size multiple times, each query will have a different instance id)
  - const size like u64
- Query type id
  - This actually says what type of query the query is
  - This would distinguish between things like: QueryName vs QuerySize
  - const size like u64
- Query inner data (Optional)
  - This would be defined by the query type

It might make more hierarchical sense to put the query type id
before the instance id, but I think practically it makes more
sense to do instance id first, because instance id should be read
even if the query type id is unknown.
(it really doesn't matter, even though my reasoning is kind of bad)
Oh, another reason is that if I did have query bundles,
and stored query type hierarchically in the raw data,
then it would be better to have the instance id first
(even though storing hierarchy would be inefficient).

Response raw data will contain:
- Packet length
  - already handled by the byte writer and reader
  - const size
- Packet type
  - To say that it is a response
  - For all requests, this should be a constant value
- Prompt Query instance id
  - Would be the same as the query instance id that prompted this response
- Response type id
  - This should be easy to tell from the query, so I am considering just not having this
  - Will have a special id reserved for unknown queries
  - If the response has a wrong type id that isn't the null id, then panicing will be understandable
  - const size like u64
- Response inner data (Optional)
  - This would be defined by the response type

For unknown queries, the server should just give a response with
a special `Null` response type id (probably like 0).
I considered having a `NullResponse` packet type, a `Null` response type id,
no response type id and inner data on the null response,
or just having an additional boolean represent
whether the response is null or not.
```

With the new terminology (packet type -> packet category, query/response type id -> packet type id) and new ordering (put query/response type id above prompt query instance id, to standardize this between the different categories), the new description should be:

Query raw data will contain:
- Packet length
  - already handled by the byte writer and reader
  - const size
- Packet category
  - To say that it is a query and not a request (other options, that don't make sense are event and response)
  - (I could eliminate this by just saying everything is a query)
  - For all queries, this should be a constant value
- Query type id
  - This actually says what type of query the query is
  - This would distinguish between things like: QueryName vs QuerySize
  - const size like u64
- Query instance id
  - Unique to each instance of a query (eg, even if you query size multiple times, each query will have a different instance id)
  - const size like u64
- Query inner data (Optional)
  - This would be defined by the query type

Response raw data will contain:
- Packet length
  - already handled by the byte writer and reader
  - const size
- Packet category
  - To say that it is a response
  - For all requests, this should be a constant value
- Response type id
  - This should be easy to tell from the query, so I am considering just not having this
  - Will have a special id reserved for unknown queries
  - If the response has a wrong type id that isn't the null id, then panicing will be understandable
  - const size like u64
- Prompt Query instance id
  - Would be the same as the query instance id that prompted this response
- Response inner data (Optional)
  - This would be defined by the response type

...

I will get started on implementing the `PacketUnion` macro tomorrow.
I think the course of action will be to:
1. expand the `PacketUnion` derive macro in `standard_packets`'s `DisplayEvent` (currently implements `Datable`),
2. manually modify it to let `DisplayEvent` implement `PacketUnion` instead,
3. then use that as a reference to update the general `PacketUnion` macro.

That takes care of packet unions of packets, but not packet unions of other packet unions.

(Some of my friends think I commit too much just to boost my git statistics.
Well I could commit right now, but I'm not, so take that, glolichen.)

Actually, I will commit right now.
Not for git statistics (maybe just a bit because number go up, monke brain go "ooh ooh aah aah"),
but primarily for organization and incremental progress.

2025/03/27

Chorus got let out early (8:30), so I am going to try to squeeze in a commit.

The PacketUnion is actually much simpler than the datable counterpart,
it is just match statements now.

Old expansion (ignore the `const _` boilerplate):

```rs
#[automatically_derived]
impl ToData for DisplayEvent {
    fn to_data(&self) -> Vec<u8> {
        let (id, inner_data) = match self {
            Self::UIEvent(inner_packet) => (
                <UIEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Resize(inner_packet) => (
                <ResizeEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Focused(inner_packet) => (
                <FocusedEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Unfocused(inner_packet) => (
                <UnfocusedEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Close(inner_packet) => (
                <CloseWarningEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
        };
        let id_bytes: &[u8] = &id.to_be_bytes();
        [id_bytes, &inner_data].concat()
    }
}
#[automatically_derived]
impl TryFromData for DisplayEvent {
    fn try_from_data(data: &[u8]) -> Option<Self> {
        let (id_bytes, inner_data) = data.split_at((PacketId::BITS / 8) as usize);
        let id = PacketId::from_be_bytes(id_bytes.try_into().unwrap());
        match id {
            <UIEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::UIEvent(UIEvent::try_from_data(inner_data)?))
            }
            <ResizeEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::Resize(ResizeEvent::try_from_data(inner_data)?))
            }
            <FocusedEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::Focused(FocusedEvent::try_from_data(inner_data)?))
            }
            <UnfocusedEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::Unfocused(UnfocusedEvent::try_from_data(inner_data)?))
            }
            <CloseWarningEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::Close(CloseWarningEvent::try_from_data(inner_data)?))
            }
            _ => None,
        }
    }
}
```

Manual fixed implementation:

```rs
#[automatically_derived]
impl PacketUnion for DisplayEvent {
    fn packet_to_data(&self) -> (PacketTypeId, Vec<u8>) {
        match self {
            Self::UIEvent(inner_packet) => (
                <UIEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Resize(inner_packet) => (
                <ResizeEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Focused(inner_packet) => (
                <FocusedEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Unfocused(inner_packet) => (
                <UnfocusedEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
            Self::Close(inner_packet) => (
                <CloseWarningEvent as PacketTrait>::PACKET_TYPE_ID,
                inner_packet.to_data(),
            ),
        }
    }

    fn packet_try_from_data(packet_id: PacketTypeId, packet_inner_data: &[u8]) -> Option<Self> {
        match packet_id {
            <UIEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::UIEvent(UIEvent::try_from_data(packet_inner_data)?))
            }
            <ResizeEvent as PacketTrait>::PACKET_TYPE_ID => {
                Some(Self::Resize(ResizeEvent::try_from_data(packet_inner_data)?))
            }
            <FocusedEvent as PacketTrait>::PACKET_TYPE_ID => Some(Self::Focused(
                FocusedEvent::try_from_data(packet_inner_data)?,
            )),
            <UnfocusedEvent as PacketTrait>::PACKET_TYPE_ID => Some(Self::Unfocused(
                UnfocusedEvent::try_from_data(packet_inner_data)?,
            )),
            <CloseWarningEvent as PacketTrait>::PACKET_TYPE_ID => Some(Self::Close(
                CloseWarningEvent::try_from_data(packet_inner_data)?,
            )),
            _ => None,
        }
    }
}
```

Yooo, I finished in 20 mins, not bad.

...

I'll work on resolving the remaining compile-time errors.

I'll make `EventPacketUnion` and `RequestPacketUnion` macros that also automatically call the `PacketUnion` macro.

YOOO, passed all tests first try.
(That might just mean my tests aren't thorough though)

...

The running also ran normally first try.
I'll make the packet union macro support inner packet unions.
I'll do this by making the user specify the attribute.
In the case the id doesn't match any of the outermost packets,
just see if any of the sub unions return a some.
I wish there was a more compile time-ish way to do this,
where the packet union leaves a special meta thing, but whatever.

Manual implementation:

```rust
// #[derive(EventPacketUnion)]
pub enum Event {
    // #[sub_union]
    DisplayEvent(DisplayEvent),
}
impl PacketUnion for Event {
    fn packet_to_data(&self) -> (PacketTypeId, Vec<u8>) {
        match self {
            Self::DisplayEvent(inner_packet_union) => inner_packet_union.packet_to_data(),
        }
    }
    fn packet_try_from_data(packet_id: PacketTypeId, packet_inner_data: &[u8]) -> Option<Self> {
        match packet_id {
            _ => {
                DisplayEvent::packet_try_from_data(packet_id, packet_inner_data)?;
                None
            }
        }
    }
}
```

...

I just realized, the try operator doesn't work like that.
It almost works the exact opposite of what I want it to do.

```rust
// #[derive(EventPacketUnion)]
pub enum Event {
    // #[sub_union]
    DisplayEvent(DisplayEvent),
}
impl PacketUnion for Event {
    fn packet_to_data(&self) -> (PacketTypeId, Vec<u8>) {
        match self {
            Self::DisplayEvent(inner_packet_union) => inner_packet_union.packet_to_data(),
        }
    }
    fn packet_try_from_data(packet_id: PacketTypeId, packet_inner_data: &[u8]) -> Option<Self> {
        match packet_id {
            _ => {
                if let Some(inner_packet_union) = DisplayEvent::packet_try_from_data(packet_id, packet_inner_data) {
                    return Some(Self::DisplayEvent(inner_packet_union));
                }
                None
            }
        }
    }
}
```

Sweet, worked first try.

...

I guess I got a bit sidetracked with all the packet union macro stuff,
since [#16](https://github.com/mathkimchi/singularity/issues/16) is about plugins.
I should've made a new issue, but I felt kind of guilty about continuously switching between
singularity issues without saving progress recently.

But this kind of was needed, because I want to define a `StandardEvent` in singularity sap's
standard packets while dividing it up into the different event types.

REVIEW: (I was thinking about it, and since I made a new set of functions for PacketUnions,
I might've just made seperate things for packets and unions where the packet itself checks
if the packet id matches. That reduces code repetition, which is good, but it also increases
flexibility which is usually good, but in this case I worry that increased flexibility
will make it easier to make errors.)

I am adding one more macro, the `Query` macro, which is very basic.
While writing it, I realized I misspelled identifier 20 times in just the macro lib.rs file.
I might try running a spellchecker on the whole codebase.
I'm not sure if there will be more typos in my entire source code, or in this md file.

I tried to look for the documentation on matching token streams and how to use attributes,
and sadly, the proc macro world of rust is very sparcely documented.
Making a MathKimchi video on how to do proc macros might not be the worst idea.
I could show off my workflow and tips that I used in singularity.

Okay, I've been working straight from 2:40 to now (4:00) as well as earlier today,
so I'll commit now and not touch singularity until I finish all my homework.

2025/03/30

I want to support communication between singularity applets now (I like this new term `applet` because it encompasses tab, plugin, and subapplication in my mind).
IAC (inter applet communication, like IPC) should come in two types:
- Broadcasts
  - Reciever chooses the sender (or the category of broadcasts to listen to that can be sent by anyone (make sure to ignore broadcasts send by self))
  - Multiple recievers allowed
  - Logical flow: reciever sends a subscribe request, for each broadcast message the sender sends a broadcast request and every reciever gets a broadcast event
  - Sender sends something analogous to the server's `event`
- Direct Communication:
  - Sender chooses reciever (sender can send `query` or `request`)
  - Logical flow:
    - Sender sends `IACQuery` or `IACRequest` to server
    - Server sends `QueryRecievedEvent` or `RequestRecievedEvent` to reciever
    - If a query was sent, then the reciever either sends back a `IACResponse` to the server, which is actually a request. (Technically, it could just not respond and screw everyone over. A cooperative reciever would at least reply with a `QueryUnknownResponse`. I'm actually going to ignore `IAC` queries for now)

In either type, when one applet chooses the other applet, they use the other applet's `Id`.
In practice, the server would offer queries and stuff to help find other applets.
TODO: store applet types as well? (some would have to be anonymous though by nature)

I wish I was careless about resources.
I mean, in contrast to other processes, the overhead of having a bunch of connections
(less than 100 for normal usecases) should be insignificant.
But, for some reason, a primitive instinct is prohibitting me from implementing the "elegant solution".
Let me describe things I would want if resources were not a problem:

- For each query from applet to server, instantly give back a seperate one-time channel for the response so they can choose to wait and listen now or just check back later. Then, disregard like a burner phone.
- A broad listener applet would be like mpsc for each type of thing to listen to
- For each direct communication, start a new channel
- Each applet instance has an applet channel (like now), but for each display it creates, a new display chat is created for all communication regarding that display (eg: DrawRequest, KeyPressEvent, and SizeQuery would be sent via display chat).

The hope would be that those ways are safer and make more logical sense as opposed to using id's to try to pack everything into one stream.

I looked at [this reddit post](https://www.reddit.com/r/rust/comments/7i4ljy/question_mpsc_over_mapped_memory/)
and [ipc-channel](https://github.com/servo/ipc-channel) by servo looks literally perfect for my usecase.
It is literally begging me to use it.
An active repo (last commit just 27 days ago), used by an established organization, almost 1000 stars,
seems perfect for my usecase, but idk.

2025/04/01

Happy April Fools!

I am going to split the project files into `ProjectConfigs` and `Session`.

2025/04/02

I am going to further refactor the project files,
to give uuid's for each known applet type.
...
Actually, it might be better to identify applet types by more consistent/deterministic method,
because there is a level of continuity between the `file_manager` applet in one project and in another.
Currently, I am just using a string like `"file_manager"`.
I could also use a more complicated datatype, (eg: struct to also store version)
(eg2: enum to differentiate different types like UnixSocket connection, piped stdio, etc but this is
actually unnecessary since this should be stored in the applet list,
not as the identifier).
Or, I could use the Id map (which is not consistent) to map applet id to applet data,
but also have a seperate hashmap to map applet name to applet id
(if speed is negligable, more elegant imo to just store name in the applet data and iter to search by name).

Okay, I am actually trying stuff, and I have a few possibilities in mind:
- Generate "id" by hashing string names
  - Just assume collisions won't happen
- Generate "id" as a constant in crates, the same way I did PACKET_TYPE_ID
- Don't use "id" just use names
- Store a mapping of names to ids, and generate id non-deterministically

2025/04/03

I think the optimal way to go is do everything based off the name, so if I do use id's,
I would make it solely generated from the string.
I decided this because I was thinking about url's and git branches.

I'm probably going to use just strings to identify applet, but I might add a wrapper for type-safety.

2025/04/13

After working on this for a few days, I resolved all the compile time errors but it still doesn't work.

2025/04/15

Oh no, I think the actions aren't showing because of numlock, which I turned on with my external keyboard,
but right now my laptop keyboard doesn't have that toggle.
For now, I just erased NumLock from my code, since it is kind of useless anyways.

2025/05/19

I haven't worked on this for a while.
I wrote down [some ideas in the thread](https://github.com/mathkimchi/singularity/issues/16#issuecomment-2888798002).

I think wasm or dynamic libraries's are worth looking into for reactive plugins.

...

Okay, I was looking through my code, and I remember what I was doing now:
I was working on IAC.
I was implementing the `Basic Features`.

2026/05/20

I gave an LLM the last github issue I wrote, and it suggested using dynamic libraries.

I looked up `rust plugins with dynamic libraries` on google,
and accidentally discovered a crate called
[`dynamic_plugin`](https://docs.rs/dynamic-plugin/latest/dynamic_plugin/),
and it seems like what I want to do but it only has 4000 users so I won't use it.
Bevy also has a [dynamic plugin crate](https://crates.io/crates/bevy_dynamic_plugin).
(I think this is different from that other Bevy thing I was looking at; maybe not)
I will dig through both crates' source code to see what they are doing.

dynamic_plugin is actually pretty cool.
The main library only uses a few crates: `libloading`, `thiserror`, `libc`, and `sa` (static assertions).
Actually, the main library is nothing and the actual value seems to come from the macros.
The macros are too big brained for me.
TBH, there is a very good case for just using this crate right now.
Okay, I am going to use `dynamic_plugin` and then `cargo expand` it.

2025/05/29

I pretty much copied the [example host and plugin](https://github.com/lilopkins/dynamic-plugins-rs/blob/main/example-plugin/src/lib.rs) into [dynamic_plugin_sandbox](./dynamic_plugin_sandbox/)
and then expanded the macros, then made it more understandable.

I'll commit what I learned, and the next step would be to set this up for sap.
The template interface would go in sap,
ideally the plugin runner would be in a crate only sde used but in practice I'll probably put it in sap,
and the plugin making macro would ideally be in sde but it might be easier to have it in sap.
Maybe I could add cargo features for sap to have client and server specific code without needing a new crate.

2025/05/30

I will actually more or less deprecate the unix socket and pipes for now
while I work on dynamic library.
Also, I will ignore the waiting stuff for now.

2025/06/03

I have a problem that I can't use closures as extern "C" functions,
which makes sense because the extern "C" function expects a pointer to a function or something;
I can't rigorously explain it but it just makes sense to me.

Let me explain my usecase with an example
(I haven't tested if anything works other than checking for compiler errors, so...).
Suppose I had an app, as well as a library for printing things pretty.
Lets start with something like:

```rust
// lib.rs
#[no_mangle]
extern "C" fn print_statistics() {
    println!("The gravitational constant on Earth is 9.81m/s^2!")
}

// main.rs
pub fn main() {
    let print_statistics: extern "C" fn() = print_statistics; // in practice we'd load the print_statistics function
    print_statistics();
}
```

but now, what if we wanted the pretty print library to be able to print changing values
like a version number?

```rust
// in a shared library
#[repr(C)]
pub struct AppState {
    version_number: u8,
}

// lib.rs
#[no_mangle]
extern "C" fn print_statistics(app_state: &AppState) {
    println!(
        "The version number is {}! The next version will be {}!",
        app_state.version_number,
        app_state.version_number + 1,
    );
}

// main.rs
pub fn main() {
    let app_state = AppState { version_number: 10 };

    let print_statistics: extern "C" fn(&AppState) = print_statistics; // in practice we'd load the print_statistics function
    print_statistics(&app_state);
}
```

Now, what if `print_statistics` needed to print things dependent on a function provided by the app?
IE, what if the dynamic library needs to call a function provided by the caller?

This is still quite simple:

```rust
// lib.rs
#[no_mangle]
extern "C" fn print_statistics(f: extern "C" fn(u8) -> u8) {
    println!("The y-intercept of f is f(0)={}", f(0));
}

// main.rs
extern "C" fn f(x: u8) -> u8 {
    x + 2
}
pub fn main() {
    let print_statistics: extern "C" fn(extern "C" fn(u8) -> u8) = print_statistics; // in practice we'd load the print_statistics function
    print_statistics(f);
}
```

but now, what if the function `f` was a closure (it captures variables from its surrounding scope)?
For example, say f returned the app's version 
With Rust, we could do something like:

```rust
// in a shared library
#[repr(C)]
pub struct AppState {
    version_number: u8,
}

// lib.rs
fn print_statistics(get_next_version: impl Fn() -> u8) {
    println!("The next version will be {}", get_next_version());
}

// main.rs
pub fn main() {
    let app_state = AppState { version_number: 5 };
    let get_next_version = move || app_state.version_number + 1;

    let print_statistics = print_statistics; // in practice we'd load the print_statistics function
    print_statistics(get_next_version);
}
```

The closure `get_next_version` accesses `app_state`.
But we can't do this for dylibs,
because closures in general aren't supported,
and a pointer to such a closure would somehow need to also capture the app state it refers to.

```rust
// in a shared library
#[repr(C)]
pub struct AppState {
    version_number: u8,
}

// lib.rs
#[no_mangle]
extern "C" fn print_statistics(
    app_state: *const AppState,
    get_next_version: extern "C" fn(*const AppState) -> u8,
) {
    println!("The next version will be {}", get_next_version(app_state));
}

// main.rs
extern "C" fn get_next_version(app_state: *const AppState) -> u8 {
    unsafe { (*app_state).version_number }
}

pub fn main() {
    let app_state = AppState { version_number: 10 };

    let print_statistics: extern "C" fn(*const AppState, extern "C" fn(*const AppState) -> u8) =
        print_statistics; // in practice we'd load the print_statistics function
    print_statistics(&app_state, get_next_version);
}
```

In other words, we just make the captured state (aka context)
an argument to a static function.

Note the use of raw pointers (`*const AppState`) instead of references (`&AppState`),
even though both technically compile.
This stack overflow [post](https://stackoverflow.com/questions/71749287/why-do-most-ffi-functions-use-raw-pointers-instead-of-references)
explains better than me.
(In other words, I don't understand why. I just do it bc it is standard.)

2025-06-09

I finished the above blog-ish blob.
I should commit, but I began writing code even before writing the blog thing,
and sunk cost fallacy dictates
that I should finish writing that code before finishing.

2025-06-11

The issue I'd like to address now is letting query return a vec of bytes,
which is harder than it seems.
Passing immutable bytes as an argument is easy:
we can just send a pointer to the start of the slice as well as the length,
and we don't worry about memory safety because
the caller is still in charge of freeing the slice.
But for returning bytes,
there isn't such a simple way.
[This thread](https://users.rust-lang.org/t/how-to-return-byte-array-from-rust-function-to-ffi-c/18136)
mentions some ways.

The big problem is freeing memory.
Rust has actually sheltered me pretty well from directly
thinking/worrying about memory safety,
but I will try to explain memory to the best of my abilities.
When we call an external function,
we expect everything that function is given ownership of and creates
to be freed when that function returns (except for things it returns).
I was going to list more rules, but actually, that is kind of it.
Just the basics of ownership.
(I don't really know where I was going with that.
I was kind of hoping I'd list down the premise then think of a clever solution.)

The safest (imo) is by using an output buffer.
The external function doesn't actually return anything,
it modifies an output buffer that was given to it by the caller as an argument.
But as it stands, the output buffer limits how large the output can be.
So, we could set up an elaborate system with more functions
where the external library somehow tells the caller how long the output is going to be,
then the caller allocates an output buffer of that length,
then the external library writes to the output buffer.
Look at `uncompress` in the
[Rustonomicon FFI page](https://doc.rust-lang.org/nomicon/ffi.html).
I don't like this though.
Maybe I could do a higher layer abstraction where this is done in the background,
but it feels like I am sacrificing performance for no good reason.

The aforementioned rust-lang thread offers the solution that I want to implement.
The external function returns a raw pointer to the bytes and the length.
It is scary though, since it plays with `std::mem::forget`
and creating and dropping it from the raw pointer
(even scarier is that [`forget` is safe because Rust doesn't gurantee no memory leaks](https://stackoverflow.com/questions/74824779/why-is-it-considered-safe-to-memforget-boxes)).
The external code should provide the freeing apparatus.

Another safe way I just thought of is using files like in IPC.
This is overkill though.

...

This is implementation 1:

```rust
/// Whoever owns this object is in charge of freeing the slice this points to.
/// Don't modify this though; I don't know what would happen if you modify this.
///
/// From: https://users.rust-lang.org/t/how-to-return-byte-array-from-rust-function-to-ffi-c/18136/4.
#[repr(C)]
pub struct OwnedCBytes {
    bytes_ptr: *mut u8,
    len: usize,
    /// REVIEW: check if this is actually needed; the rustlang thread doesn't use it.
    capacity: usize,
    free: extern "C" fn(&mut Self),
}
impl From<Vec<u8>> for OwnedCBytes {
    fn from(mut value: Vec<u8>) -> Self {
        let bytes_ptr = value.as_mut_ptr();
        let len = value.len();
        let capacity = value.capacity();

        // https://stackoverflow.com/questions/74824779/why-is-it-considered-safe-to-memforget-boxes
        // this memory is leaked here but will be freed in the `free_vec` function, which is called exactly once in `drop`
        std::mem::forget(value);

        /// https://users.rust-lang.org/t/how-to-return-byte-array-from-rust-function-to-ffi-c/18136/13?u=mathkimchi
        extern "C" fn free_vec(bytes: &mut OwnedCBytes) {
            let vec = unsafe { Vec::from_raw_parts(bytes.bytes_ptr, bytes.len, bytes.capacity) };
            // no need to manually call drop, but I just wanted to highlight it
            drop(vec);
        }

        Self {
            bytes_ptr,
            len,
            capacity,
            free: free_vec,
        }
    }
}
impl Drop for OwnedCBytes {
    fn drop(&mut self) {
        // free the forgotten vec
        (self.free)(self);
        // the rest will be freed normally
    }
}
```

I liked it as an improvement of the rust thread solution,
because it was super abstract and stuff,
but it gives me the icks just a little.
I'll commit this current implementation though.

The capacity is really annoying with this.
It just feels so arbitrary.
I could further abstract into:

```rust
// worst OOP ever, lol
#[repr(C)]
pub struct OwnedCBytes {
    ptr: *mut c_void,
    get_len: extern "C" fn(*const c_void) -> usize,
    /// fills the buffer, which is expected to be length of `get_len`
    get_bytes: extern "C" fn(*const c_void, *mut u8),
    free: extern "C" fn(*mut c_void),
}
```

(`c_void` represents an opaque type)
but I don't want to do this.
This is just OOP but horrible.
(Actually, this might be useful later.)

Instead, I am going to do the opposite approach and specify for rust Vecs.
I suppose I am just going all-in on the assumption that both sides are written in rust
and will use the shared libraries provided by me.
(Given that I am likely the only person who will use and even-more-so develop for singularity,
I'd say that is a fair assumption.)

---

SIDE NOTE: I noticed I could do `ManuallyDrop::new(value).capacity`
but manual drop is defined as:

```rust
pub struct ManuallyDrop<T: ?Sized> {
    value: T,
}
```

so you'd assume the proper syntax is `ManuallyDrop::new(value).value.capacity`.
Apparently it is from the `Deref` trait.
TODO: harness this for `(pub` matches or grep for certain structs.

---

I asked ChatGPT about the memory safety of:

```rust
impl Drop for CVec {
    fn drop(&mut self) {
        // REVIEW: is this going to double free?
        let vec: Vec<u8> = unsafe { Vec::from_raw_parts(self.bytes_ptr, self.len, self.capacity) };
        // unnecessary but highlights that the vec is dropped
        std::mem::drop(vec);
    }
}
impl From<CVec> for Vec<u8> {
    fn from(value: CVec) -> Self {
        let vec = unsafe { Vec::from_raw_parts(value.bytes_ptr, value.len, value.capacity) };

        // prevent double freeing the vec in CVec's drop
        std::mem::forget(value);

        vec
    }
}
```

and now I realize I don't understand how memory works in rust.
I am going to watch a video on it.

---

I watched [Visualizing memory layout of Rust's data types](https://www.youtube.com/watch?v=7_o-YRxf_cc)
and it is pretty informative.
It doesn't go over what happens with Forget and Drop,
but whatever.

After the research,
I still am not sure if the code is safe,
I guess I will find out if it bites me in the behind.

2025-06-14

I am now implementing the applet context,
which is like the new ServerHandler.
In doing this, I realized that I should actually deviate from the `UniversalStream`
implementations.

So, I want to explain the protocols/implementations that exist for singularity:
- Datable Trait
  - `Datable`, `ToData`, `TryFromData`
  - Just a rust trait for converting an object to bytes and vice versa, where the object's type is known.
  - This allows for sending objects with pre-known types through FFI and sockets and saving objects to files.
- Packet Trait System
  - `PacketTrait` is a `Datable` with a type id.
  - An implementor type of `PacketUnion` represents a bundle of different types that implement `PacketTrait`.
  - An object instance `PacketUnion` represents a specific `PacketTrait` object.
  - Given a `PacketTrait`'s byte representation + the `TypeId` of the `PacketTrait` the bytes came from, if that `PacketTrait` is in a `PacketUnion` type's bundle, the `PacketUnion` type will create an object instance of itself.
  - Allows for sending and storing objects with multiple possible types (the multiple possible `PacketTrait` types are bundled into `PacketUnion`).
- Categorized Packet Traits
  - Just `PacketTrait`'s and `PacketUnion`'s with category (Event, Request, Query, Response) specified for additional safety.
  - `UniversalQueryTrait`, `EventPacketTrait`, `RequestPacketTrait`, `EventPacketUnion`, `RequestPacketUnion`
- Byte Stream Trait
  - `ByteStream`, `ByteReader`, `ByteWriter`
  - Deals in chunks of bytes, where the length matters and isn't constant. ('Hi' is different from 'H' then 'i')
  - Flexible reading:
    - Poll (gives all byte chunks currently readable)
    - Waits (waits until a byte chunk is readable and then returns it)
  - Writing:
    - Send a byte chunk
  - Implementation Note: many implementations send the length first and then sends the actual byte chunk
- Universal Stream Structs
  - `UniversalClientStream`, `UniversalServerStream`
  - Given a byte stream object and with recieving bundle types (eg: a `EventPacketUnion` type for the client's universal stream), the universal stream is able to send `PacketTrait` or `PacketUnion` objects and recieve `PacketUnion` objects
  - Is implemented (everything above is a protocol with open implementation).
  - The byte-level structure of the packets that I described earlier (like in 2025/03/26) are pretty much just for the Universal Stream implementation
- Dylib Applet
  - I am working on this right now. The scope of it is currently unknown.

With active applets, the SDE and applet talk to each other via universal stream,
where they use matching byte stream methods.
With reactive applets (`dylib_applet`),
I am planning on making the `dylib_applet` protocols.

In both methods, they send categorized packet traits to each other.

I am compelled to also provide specific methods to dylib applet,
for reasons like performance.

...

Dylib Applet feels so out of place in `singularity_sap`,
I felt like I should put it in its own crate.
But, I realized that rust modules exist for a reason;
I should stop making new crates and just make modules with cargo cfg features.

Side note: Wowie, 10k lines of singularity? Noice!

...

I will add a `ratk`,
which will be the `reactive applet toolkit`

2025-06-16 1:17AM

I finished writing the `register_applet` macro.
I haven't tested it yet (that is the next step, to make a dylib client).
The macro itself is very simple, and the bulk of the time I spent on
developing today was avoiding code repetition by implementing packet to and fro `typed_data`,
as well as figuring out lifetimes for `CBytes`.
I also tried to clean up some code.

I don't know why I am trying to write down excuses for why I worked unreasonably
long on this commit.
If I had to guess, it is probably because I am trying to cope with the fact
that I tried to multitask watching Last Week Tonight while working on singularity
for an hour or two (or three possibly, sue me) and realized that
I should only code until I finish this commit,
and then I can focus entirely on whatever else I want.
Welp, now it is 1:25AM, and I am starting a lab tommorow
(technically today, 2025-06-16).
I want to wake up at 7AM, eat breakfast, and get to the lab early, like 8:30AM,
even though for the last 3 weeks of summer, I've been sleeping after 2AM and waking up at 12-3PM.

I wanted to finish a course of 16, 1 hour lectures before starting the lab,
but I think I watched the important parts, so that won't worry me to the point of not sleeping
(instead, that will simply be my inconsistent sleeping habit).
Anyways, I guess am kind of stalling now because I need to shower,
and I kind of don't want to, even though once I start showering I enjoy the warmness of it.

It is weird that I hijacked this devlog entry with personal info.
I already write relatively a lot about my personal life in my devlogs,
but this is more than usual.
I am not sorry for adding this entry to the singularity devlog,
but I am feeling dejected
(it's the feeling of to sighing and saying whatever then reluctantly following along with what is happening,
but it's weird since I am the one who is making it happen),
because I have a seperate journal specifically for personal thoughts,
as well as a seperate diary for events in my personal life,
and I am currently messing up the organization of those journals.

But, once I get the documentation/devlog/journal/diary writing applet working,
these organizational problems will be of the past.
(HA, NOICE!
I was able to make this entry somewhat relevant to singularity,
thereby justifying its location!
Truly, a genius maneuver.)

... well, now this entry does belong here,
so me talking about how it ruins my organization should be removed,
but if that is removed, then the entry wouldn't be relevant to singularity,
so I could add back the ramblings,
which would make this entry relevant again.
But then, I'd need to...

Whatever, I'm "going to sleep" now.

2025-06-16

To test dylib applet, I will make a math game.

I wanted to do a sudoku or maze game,
but the logic for both of those is unnecessarily complicated
for a simple demo.

Also, I noticed that all my crates are still 2021,
so I will update them to 2024, which came out a few months ago.

2025-06-19

I am going to write two sets of functions in the `ReactiveApplet` trait
for global events vs instance events.
I also might want a seperate function for creating an applet.
I also might want two seperate events for global vs instance.

2025-08-09 10:16PM

WOW, it has been a while.
Over summer, I have been working on an internship, college apps,
and for my personal project I've been working on celldom.

I have been thinking about singularity, but evidently, I haven't been coding for it much.

I remember I was doing something with dylibs, but I don't know what this commit was.

I don't want to use VSCode, but neovim and emacs are annoying to configure with Nix,
but this is good because it will incentivize me to work on singularity even faster.

I guess I should start by looking at what I've modified and maybe some TODO, REVIEW, and FIXMEs.
Maybe I should actually start by looking at the logs.

Okay, so it seems like my previous commit was me writing a play.
Then, on 2062-06-16, I said I would make a math game to *test* the dylib in this commit,
so it seems I already wrote code for the dylib infastructure.
Then, I updated my crates to rust 2024.
On 2025-06-19, it seems I was going to make two different function typesfor global vs instance events,
and I am not sure if that is still a TODO.
Knowing me, it is probably still a TODO.
On 2025-08-09 10:16PM, it seems like I started logging the time of day in the devlog so I could brag about having no life.
I commented on how I haven't been working on singularity and gave some half-baked excuses.
Then, I babbled a bit about random stuff, and then I started to look at the logs and summarizing them.
Then, I finished singularity, did a backflip, and made it in time for my daughter's ballet recital.
Oh wait, the previous sentence is all halucinated, the most recent thing I did is look at the logs and summarized them, before I took (or, am taking) this chance to become meta.

2025-08-16 8:16PM

Uh, after writing my last entry, I didn't actually do anything.

First, I have to deal with deallocating an Applet instance from memory.
I am just going to add a function called `close_applet` that takes ownership of the Applet object
and therefore is responsible with deallocating it.

2025-08-24 6:03PM

I have to write code to have the sde actually use dylib now.

...

Maybe its because I just lost PeddieHacks or because I am coming back to singularity after so long
or maybe I am just seeing this from the correct perspective for the first time,
but my code is too messy.
I might do a soft restart/refactor by first figuring out what modules are solid,
where the solid modules belong,
and then rethinking the architecture of everything that remains.
I should be able to explain what every single thing's purpose is
by only looking at the name and what module its in,
without needing to read documentation or code.

Either way, I am just going to commit this current mess right now.

...

First, I think that the "util"s are most solid and clearly defined.
Most of `singularity_sap`, `singularity_common` (which only consists of utils rn),
and `singularity_ui` (there is a lot of improvements to be done, but at least it is pretty clear what it is supposed to be) are fine for now.

I want to redo the rest and `singularity_sap::dylib_applet`.

2025-09-20 12:02AM

I have been doing a lot of thinking, and I wrote some stuff down on my paper journal
as well as brainstorming UI on an iPad.

To summarize the big points:
- My progress on singularity has been abysmal because I keep adding "widening" it without following through on a single Minimal Usable Product (I prefer this to MVP because my current sights are set foremostly on getting a usable product ASAP). I have been wandering aimlessly in my codebase adding things that are easy to implement (or at least seem easy to implement).
- I have begun to "manifest" singularity, where I try to imagine using singularity as intensely and vividly as possible. With this, I aim to get the "what" of sinularity solidified such that my worries on the "how" can be directed towards forwards progress. This contrasts to the past where my lack of vision or the "what" has made me implement the "how" in useless directions.
- Steps I will take now:
  - I will commit this and close this branch and issue.
  - Plan new crate structure
  - Purge all subpar code
    - (Even the good ones that I don't absolutely need right now can be purged now and added back later)
  - Most basic applet
    - Only predetermined events
    - Plugins are static-time Rust plugins

I do already have many ideas from my manifestation sessions, but in lieu of my limited time,
I will leave those on my iPad until I begin coding them.

I wanted to call this new Singularity the Nova Singularity,
but I realized that there isn't really an old Singularity to compare it to,
since I am still working on the first MVP.
I guess I can call this the reignition stage of the development era.

2025-09-20 11:24AM

The crates I need are:
- UI
  - Current one is good
- SDE
- STTK
- Some tabs (standard tabs)
- Sporg (but this is kind of a mess as well, might need to start it from scratch.)
- and the common just for help

2025-10-09 2:20AM

I will delete unnecessary crates.

2025-10-11 9:12PM

I'm going to change the code now to get rid of unnecessary files and maybe add some skeleton.
I want to get rid of errors from referencing code that doesn't exist.

Things like the packets, I will put in singularity_common.

Packets (both the abstract protocol and the standard packets) used to be in SAP.
I am getting rid of abstract packets while Singularity is still in pre-alpha.

I think I will do reactive applets as the only way for now.
I won't support processes or multithreading either in pre-alpha.

2025-10-12 9:27PM

I will start out by planning the Handlers,
which represent communication between the SDE and applet.
The `ServerHandler` is used by the Applet and implemented by the SDE.

After thinking about it, I don't need a Ratk as a seperate thing,
because I can just have an `AppletHandler` in singularity_common
(and I'll call it `ReactiveHandler` since that is more clear).

I guess if progress wasn't a priority, I'd put both of these things in singularity SAP,
but it shall not exist while I continue forward.

Actually, I am just going to make sap a folder in singularity_common,
since I was going to make a folder for all the handler and related things anyways.

My vague idea for communication:

- Applet -> SDE:
  - Requests (make queries=requests later?)
  - Queries ask for a response in return
  - Creating other Applets:
    - Make the applet instance on its own and then just give it to the SDE and tell it to register it. (a type of Request)
    - Could also have a more generic/abstract way, but idk how yet. Would still be a request.
- SDE -> Applet:
  - Event: inform applet of event that happened
  - Response to queries
  - Initialization: different from a normal event (currently all events are instance events, no global) because this is called globally and also requires return value

I guess the new thing is the initialization.

2025-11-11 11:26AM

Lowkey, I am super lost because its been months since I actually worked on singularity.

2025-11-12 5:04PM

I explained to the other guy in Micro47 the overview of Singularity.
The main issue is just connecting Applets to the Singularity server.
The four levels of this I've considered (top is most general and difficult to rigid but easy):

1. Processes
   1. Dynamic
2. Dylibs
   1. Dynamic
   2. Reactive
3. Threads
4. Reactive Rust Objects
   1. Reactive
   2. Server Context Object:
      1. The server talks to applet by calling `applet.some_method(..., server_ctx)`
      2. The applet talks back to server by calling `server_ctx.some_method()`
      3. The server passing context as a bundle of callbacks
      4. Metaphorically, when the server calls an applet's method, the server is sending a letter/order to the applet. Including the context is like signing the letter "You can call me for more information or help at _"

I think I had something not bad for threads.
Then, I was like "I want a challenge" and did dylibs.
Then, I got annoyed by how slow I was implementing dylibs and switched to Reactive Rust,
but gave up after that.

So I tried to take 1 step forward, took 2 steps back,
then just laid down.

The really cool thing about Reactive Rust Objects is that maybe applets could have subapplets,
but I don't want to think about that right now.

I am going to try to run through a very basic flow
(simplification of projects for now. Also, tree nodes store views not tabs):
- User starts singularity for the first time
- Singularity creates a tree with just the root which is a view of a hub applet
  - (Hub is like a mix between a terminal and the vscode command palette)
  - Creating a hub applet:
    - Server runs `Hub::new()` and should get `Some`
- User types in: `replace_self(apps.find("EDITOR"))` (syntax is just placeholder)
  - Server relays user event to applet like `applet.handle_event(user_event, server_ctx)`
  - Applet gets the typing information and stores it in its current string or st
  - Applet updates its ui display with `server_ctx.request(...)`
- When the server tells the applet the user's `<ENTER>`, the hub applet spawns an editor child applet
  - Applet runs `let editor = Editor::new().unwrap();` (returns a blank editor without a file)
  - Applet calls `server_ctx.request(Requests::ReplaceSelf(editor))`
- Server replaces hub with applet in the view
- User types stuff
  - Already know how to deal with this
- User closes the editor
  - Server calls `applet.destruct(server_ctx)` (which takes ownership)
  - In production, there should be a way to ask confirmation on non-forced closes, but ignore that for now. Pretend the app just saves the text to instance storage
  - Applet calls `server_ctx.request(...)` to update instance storage
  - Server updates applet's instance storage
  - Applet ends everything it needs to (shouldn't be much)
  - Server removes everything else related to applet (or puts it in the history)

2025-12-03 11:46PM

I don't have anything technical to write right now.

But, I wanted to write down that maybe I should grab myself a whiteboard,
a good hour or two, and maybe a friend/rubber ducky and re-draft all the
types I need as well as the interactions between them (in the form of typed functions).

Oh, I guess I can also say something I thought about the actual tree-hierarchy system of subapps.
An idea I had is that I could do a recursive style implementation of the tree hierarchy.
What I mean is, that SDE (or some part of its framework) could technically
just be a single-applet runner.
But, in practice, most applets can implement `NodeApplet` which would
allow the implementer to implement the Applet itself,
but the Applet itself is enclosed and the NodeApplet has code that allows for it to be a parent
node as well.

This idea is the culmination of a series of smaller change ideas:
1. Just have a 1-to-1 correspondence between a tree node, applet instance, and a view.
2. But what if I want two applets in one view? Create a TilerApplet that is a parent of those two. The TilerApplet's implementation is that it's view is updated by having one half be one child's view and the other side be the other child's view.
3. But this sounds super wasteful if done through the same framework as all the other applets and I want to avoid adding a special exception just for a TilerApplet. Well, what if we made every applet recursive by nature?

Making the entire tree-hierarchy of the applets come from recursion is the crux of the idea.
Most Applets will run inside a NodeApplet,
where the NodeApplet library handles all the tree-related stuff
like setting focus & view and forwarding events/views between SDE (via parent) to
the focused tab (either self or the focused child).

This *is* kinda scary because if one parent messes up the hierarchical duties,
then all its children will also stop working.
But, I think this is fine if I make NodeApplets the default
and people have to go out of their ways to do something else like a SplitView Applet.

Right now, I am leaning towards SDE is just an AppletRunner so I might call it SAR
or something,
and the tree selection stuff is implemented by maybe the RootNodeApplet
which is like NodeApplet but with extra root-related duties
like the tree traversal/selection.

2025-12-04 11:11PM

I am going to just ignore tab restoration for now.

I will have a Singularity App Runner, SAR, instead of SDE.
I will just start by having the SAR and a basic text editor.
SAR will be like Singularity UI where it could theoretically be a general
crate for non-singularity app development.
IE it will be a general app framework
and all the singularity specific stuff (like hierarchy)
will be implemented in the applet toolkits.

2025-12-05 5:34PM

As I am working on the skeleton code, I am not sure how to do the UI.

The obvious plan I had was to just let them update with Queries and whatnot
since I am just doing reactive applets.

In theory, this should be fine but my gut,
motivated perhaps by long-term planning or perhaps greed and ambition,
is telling me I can and should do better right now.
The problem with the reactive approach is that is has absolutely no way to work for active applets.

The best alternative I've thought of right now is by thinking of the applet and SAR relationship
through a new perspective:
instead of SAR holding/owning the applets,
the are two independent things (they don't own each other)
and each have ways to call each other's functions.
If I can make this work within Rust,
then it will be efficient for reactive applets while also being fexible.

2025-12-06 9:10AM

The first thought I had was to use MPSC, but I want it to be even more flexible than that.
Maybe just functions where MPSC is a default implementation.

I don't know how the ordering would work for this.
The naive way is to first create one (of the SAR or the applet) with an Option for communication
and then make the other object and the communication and give it to the original thing.

But I don't like the Option because we know that once everything is initiated,
it will be a Some but we will need to keep calling unwrap.

Another order would be to start by creating the connection and then create the two objects (like MPSC).
But if I do that, then it wouldn't be very flexible.

Or, I could do something slightly similar to the Option idea but just start them out with trivial methods
and allow it to be modified.
I think the term for this is hooks.

So I think I could just have it so that the order is:
1. Create an Applet which has methods: `set_hooks` and `handle_event` and `get_display`, ...
2. Create the SAR and give it the applet
   1. SAR makes everything it needs to
   2. SAR makes hook that implements things like `notify_update_display` from itself
   3. The SAR calls `applet.set_hooks` and gives it the hook

This is actually similar to the reactive applet where I send the server_ctx
as an argument every time the SDE called an applet's funciton,
like `handle_event(event, server_ctx)`.

I think I can assure myself that creating Applet first makes sense
because an Applet should be allowed to exist without a runner,
but a runner needs an applet to run.

Hmmm...
I am not sure how the ownership would work with this
because Hook holds a reference to SDE
but I don't want to deal with lifetimes and all that mess.
I guess MPSC "dealt" with this problem by letting the user not worry about lifetimes
and with no gurantee of the reciever's existance,
but then you get an error if the reciever doesn't exist.

In the current (above) idea with the hooks,
I simply replaced an Optional hook by allowing a trivial implementation instead of a None.
But, here is another perspective:
we can kind of "drag out" the two states of the hook to say
that the Applet itself has two states: an applet with a hook and an applet without a hook.
The `set_hooks` function essentially consumes an applet without a hook
and returns an applet with a hook in its place.
So, what if we just make the before vs after two different functions entirely?
This idea is the same as just having an applet initializer function.
So, the steps taken would be:

1. Create Applet initializer data
2. Create Runner and give it applet initializer data
   1. Runner makes its own things that it needs to
   2. Runner makes the hook
   3. Runner initializes the Applet, giving it the hook

I actually vaguely remember doing this exact thing in the past.

...

The only communication between the applet and applet runner seems to be UI stuff
when the Applet Runner is just SAR and doesn't implement hierarchy.

With the recursive approach,
if I set up the Applet to specifically be used by SAR
and without any singularity stuff (like the tree hierarchy) in mind,
then I might end up needing a new more flexible/versatile trait
for apps being used in nodes and stuff like NodeApplet.
I guess that wouldn't be the end of the world,
but it feels like it defeats the point of having a recursive thing.
I will allow it for now.

I haven't explicitly said this yet,
but one of the things I'd like to support is Components like textboxes through the Applet
framework.
But I think the applets' flexibility for being able to call the hook whenever
might be kind of annoying when we know that the display should only update reactively.
I could definitely make it work with MPSC and Mutex,
but that is unnecessary resources and I will look for a simple way of doing it later.

I guess I could start coding by simply making a textbox.

...

Oops, I forgot to update skeleton code to have the initializers.
While I am changing that, I am also going to change the name for Applet to be BasicApplet.

2025-12-12 1:53PM

Implementing the runner logic and a standalone app was very easy.
Now, I will implement the singularity hierarchy logic in sttk,
but I don't know if that is the best place for it.

Actually, I will implement ending logic before that
(I was cleaning up my code and realized I should add this).
I could do some ownership stuff with this, but I don't see the usecase right now.
I think drop is enough as well, as long as the user doesn't call any hooks on drop.

2025-12-14 11:33PM

Now, this is the real test: the singularity hierarchy logic.

The naming I will use is to say `BasicApplet` is the bare minimum needed to run and talk to the UI,
and `NodularApplet` for the applets that support hierarchy operations.

I am slightly bummed that the recursive approach doesn't really work with the ID system,
but I acknowledge it could actually be an opportunity for theoretical purity.

Ooh, I just got tingles from this idea:
*IF* I somehow devise the interfaces such that the only differences is that the hook has more things,
then this would be really good in rust
(I don't want to explain explicitly, if you want to know why, then try implementing it yourself and you'll see).

Well, now I have to actually see if I can do that.

Recap of the basic applet:
- Applet calls:
  - `handle_ui_event`
- Runner hooks:
  - `update_display`
  - `close`

For the nodular applet, I plan on having a mini view which is a generalization of tree view.

Nodular applet:
- Applet calls:
  - `handle_ui_event`
  - `handle_nodular_event`
    - For things like focus and highlighted
- Runner hooks:
  - `update_display`
  - `close`
  - `update_mini_view`
  - `add_child`

So unfortunately it seems like I need more applet calls.

Or maybe I can somehow seperate the mini view logic from the normal stuff.
The reason for me wanting to generalize the mini view logic is because of a usecase like a markdown editor with sections.

Maybe I can brainstorm the seperated mini view later,
but I guess I'll just pursue the naive approach right now.

2025-12-21 2:46PM

Freak...

I just realized that some tree operations might be very annoying to implement recursively.
Some might require global coordination from the root and the recursive implementation would
just be a very contrived way to execute a globally coordinated algorithm recursively.

Without thinking about it too much,
I think this goes against the spirit of singularity.

Let me commit the (atrocious) code I wrote so far in this commit and contemplate further.

2026-01-01 12:42AM

New year, new singularity!

2026 will be the year of the singularity.

Jokes aside, as I reflect on how long I've been "developing" singularity for,
I can not mask my disappointment.

If anything, I want 2026 to be the year of reflection and new directions in the context of
singularity's development.

That said, I decided to push through with the recursive subapp implementation
and to address the flaw exposed by globally coordinated tree operations,
I just won't address them (for now).
I won't even worry about the minimap.
For now, I will just implement two non-leaf applets:

1. The standard RecursiveNodeApplet (just shows the focused item)
2. The DividedApplet is like a very limited tiling window manager, which, like the standard node applet, holds one main subapp and a list of children subapps. All the subapps are given equally sized rectangles, and it can switch between horizontal vs vertical stacks. This is mostly here right now for debugging purposes.

I think all the tree traversal operations I've previously implemented can be done locally,
and the only things that needed global coordination was tree modification.

2026-01-07 11:57AM

Bruh, I am just spamming `Arc<Mutex<Box<T>>>` everywhere.
I feel like I surely have a circular reference somewhere
and it is very ugly.

2026-01-08 10:03AM

The code makes me want to puke.
Because of Rust's ownership rules, it is really hard to reuse functions in this case.
Specifically, I was avoiding giving a reference to the parent (RecursiveNodeApplet) to the hooks,
because the parent holds the child which holds the hook so if the hook holds the parent,
we get a circular loop.
There might also be a problem with Mutex getting infinitely stuck.
But this means that I have to isolate everything that the hooks need to hold.
So certain operations like updating the window to be the focused node
has to be implemented 3 times
(once for children, once for the primary child, and once for the focused node itself).

I am going to look into weak references as well as creating
bundling every shared object into just one type that is held by the hooks and parent.

2026-01-09 9:22AM

I have decided it will be easiest to start with `DividedApplet`
without a special main child.
I will add that later though.

2025-01-16 11:58AM

I have invaded a random Stats class because I got a class off,
and I feel so productive.
I finally squashed the multiple deadlocks I had,
and surprisingly everything just worked smoothly from there.
I am a little worried that the chances of deadlocks will only increase as
the project gets more complicated.
If that happens, I might end up just putting everything in its own thread,
though that would make this architecture useless.

Well, I am going to commit what I have.
This is the first time I am running an app that holds another app.

Now, I am going to give the holder two applets and switch between them.

2026-01-19 10:04PM

A system where applets return the display instead of updating shared memory
would be philosophically more elegant,
and would trivially support the updating shared memory method,
but in practice, this would be bottleneck paradise.

Anyways, I want to implement the tree hierarchy aspect of singularity.
Specifically, the UI of it: the Quick Map.
I have kind of been dreading this, because this is the make-or-break aspect of singularity.

I sketched out an example of what this would look like on my iPad.
In the final form, I imagine two main parts:
the treeview and the selected view preview.
There will also be a plucked root view.
But the main feature is the treeview, so I'll implement only that for the MVP.
(Treeview without space like preview; I'm making up this word.)

The fact that there is no elegant interpretation/explanation of the tree view makes me sad.
But whatevs.

...

1:24PM

<!-- I was not going to do this for fear of getting stuck in another rabbithole
(or rather a rabbithole I've been in before).
I am talking about the generic packets.

I might need to do this because  -->

I am going to generalize the multi-app holder.

First, I should generalize the Mutex UI.

2026-01-21 11:56AM

Yesterday, as I was organizing my thoughts as one does,
I had an idea for possibly optimization.
Well, I guess it is more an extension of an idea I already had.
The old idea is just the principle that if I have multiple things
in a container and then I update the things inside the container,
I don't have to update the whole thing; just the things that changed.

I simply realized that I could combine this with the hook system.
So each UI element will have a `was_updated` boolean,
and when an element is updated, its parent is also updated.
(Btw, I'd make a new struct like `EfficientUIElement`.)

Another seperate idea is abstracting on the sync logic,
which, above other benefits, can ensure no deadlocks.
The most basic abstraction is surrounding all uses of Mutexes behind objects
with methods where I can guarantee it executes fast and will not have deadlocks.
For example, just having a setter and getter (of the clone to be precise)
is the minimal implementation of this idea.

The more exciting idea I had was a sort of lazy processing.
The general task is that we want to run a sort of
clean-up function once everyone stops using it.
(The difference between this task and setting drop+Rc
is that this clean-up function is called when everyone stops just actively accessing it
not when they completely drop the object.)
In the context of singularity, this could be useful for updates that only need to happen once,
once everything else is done updating, like updating ui.
More generally, since I allow for objects to call children or its parent,
the call-stack can go up or down depths.
The clean-up for object A should be called when the last occurance
of object A in the call stack is finishing.

(I watched a video on Djikstra's Semaphores and want to make my own synchronization object.)
Let me introduce my solution in the form of a data structure:
the Clam (name in progress).
The clam holds an inner object<!-- (clonable, most likely Arc)-->,
a borrow counter (like Rc), and a clean-up hook.
(The clean-up hook *could* theoretically be static, but I'm not going to worry about that.)
You can call get_pearl on the clam, which increases the counter and
returns a pearl containing a clone of the inner object.
A pearl lets you access the inner object.
When it is dropped, it decrements the counter and if the counter becomes zero,
runs the clean-up hook.
Clam should be clonable.
Maybe I should let Pearls be cloned or even allow Pearls to
output its spawner Clam too.

Actually implementing this is annoying because I don't know
how much I actually need to use Arc for.
I don't need a Mutex around the inner because the user can specify
`Clam<Mutex<T>>` if they need it.

Also, I think I should just document a simple
principle for avoiding deadlocks where a thread waits on itself
with Mutex/RWLock:
do as little as possible while there is a lock.
When debugging deadlocks, try to look for functions that are called
while there is a lock, and avoid possibly recursive calls while
there is a lock.

In the documentation, I explained it like this:
Clam is the dormant state
(meaning you can access the object later but aren't accessing it now)
and Pearl means you are actively accessing the object.
When a pearl is dropped and there are no current pearls,
the clean-up function is called.

2026-01-22 9:53AM

I implemented the EncapsulatedLock,
which should gurantee that there are no deadlocks
by only exposing a getter function that clones and a setter.

I was also considering having something similar to Mpsc
called Mrsw (multiple reader, single writer),
which is just encapsulated lock but even more limited.

...

Now I have to figure out how to actually use my sync abstractions.

1:26PM

I was just squashing the syntax errors after adding AppletHolder
(I made AppletHolder before the last few commits until I got distracted)
and using it in recursive node applet.

On an irrelevant note, I was thinking more about a generalized UI Element.
A generalized UI Element should be able to do two things:
- draw self onto a rectangle of some size
- return whether self should be redrawn

When the element is resized, its parent should automatically call `element.redraw`,
but when the element is just translated, then maybe it would be optional.

I could also tweak this to be hook-based and also with a boolean for to_update,
which would enforce speed.

2026-01-23 9:13AM

During breakfast, the person I sat next to left
so I took out a pen and started drawing the `to_update` on a napkin.

Then, first block was CS, so my teacher wanted me to present
what I've done since my last presentation.
I just went through my commits and then we talked about IPC and theorized about performance.
I didn't show them the most recent working demo,
because I feel like if the kids found out this is the result of years of work,
they would laugh at me.
(I am not being overly self conscious, I know am in a class of jokesters.)

Anyways, the analogy I gave my teacher for
the feature I am currently working on was that instead of upwards requests being like:
"hey parent, I need you to do XYZ for me right now"
(`parent.update_display` updating the display in that call),
the new structure would make children be like:
"Hey parent, I have an update for you. Call me when you are ready to hear it".

TANGENT:
I don't know how queries are going to work in this system though,
but that is a problem for the future.
Something to note is that I am inverting the previous (like before this branch) expectation
that the child shouldn't be waited on by a parent.
With my new recursive active+passive (reactive) architecture,
I actually encourage calls to the parent to be simple.
But this inversion is quite a coincidence more than anything.
The old architecture enforced not waiting on children
for the sake of safety, to prevent a child from freezing the whole system when it freezes.
But the way I take advantage of multi-threading being unneccessary in passive apps
means that I must take the risk of waiting on generalized child calls.
I encourage upward calls to be simple to avoid self-deadlocks.

Anyways, let me be more specific on how I want to technically implement this.
Consider a minimal example where the fundamental necessary interactions are
event notifying from parent to child and update display from child to parent.
Event notifying should work as normal.
If a child wanted to update the display,
it would set `to_update = true` and then call `parent.notify_update()`
if `to_update` wasn't already false.
And the parent would do the same thing to its parent.
On the root applet's side, it would set its own `to_update = true`,
and then finish and let all the calls to the children finish.
Once we are back at the root, at the end of the root's original function,
it checks if its own `to_update` is true.
Then for all its children, if the child's `to_update` is true,
it turns the child's `to_update` to false then calls something like `get_display` on it,
and this happens recursively.
A standard applet should just store the ui already before notifying the parent,
if it is a good boi.
But, for something that is subject to changing very fast,
a lazier approach is sensible.
(The shared bool might be unneccessary,
but I am fine with it because it is helpful anyways for preventing redundat notifications.
I kind of wanted to have all downwards functions return a boolean for the new `to_update`
value, but if that was ever needed, the child can just call `parent.notify_update()`
at the very end.)

I'm not actually going to do the generalized UI thing just yet.

With this `simplify upward calls` refactor,
I am implementing a more specific solution, but it is simpler than having the whole clam.

(By the way, just want to note that I *have* been coding,
specifically I was using the Sync primitives I made,
but I am just not storing them in the git.)

...

Something I've been wanting to start but is scared to do is streaming my coding sessions
because 1: for more connvenient devlog, and 2: its fun.

I am kind of scared to do it on MathKimchi
and even considered making a second channel just for streams,
but I think I just need to stop pretending like this is some huge thing
and start without super high expectations.

2026-01-24 8:40AM

I'm not sure if I should consider the root
the outermost applet (the root applet),
or just make it the runner.

This also brings up the question of how this works with different types of updates.
I am going to do something really scuffed and say that for display updates,
the runner is doing the root stuff.
For treeview updates, I guess the app that is a basic app but holds nodular apps should handle that.

2026-01-27 8:26AM

Bruh, I finished my work early for my stats class.

I briefly looked into Wayland's protocol and I think I am getting closer to it.

The interface is:

BasicApplet:
- handle_ui_event
- get_window (I kind of use window, display, ui interchangably)
  - I am not enforcing Mutex here to support lazy apps (and not to support my laziness)

BasicRunnerHook:
- damage_window ("damage" seems to be the term Wayland uses for saying that the window should be updated)
- close

Later, I should update the UI library to match this new protocol.

...

10:21AM

The performance is obviously very bad,
but can't remember if it was always bad or this is worse.

2026-01-28 11:19AM

Enough procrastination, I should start the treeview.

9:02PM

After thinking (aka sitting on my bum),
I have concluded that for now, I will implement the treeview
by making each applet return an actual tree of strings,
not the generic UIDisplay I am hoping to do in the future.

I ask myself whether it was the right decision to refactor
what was already working.
I knew that I would waver and regret switching to a new architecture.
Yet I trudge on, not because it is too late to back out,
but because it is still too early.
Singularity deserves to be broken down and rebuilt
until it can no longer be improved.

Erhm, what the yap?

Ignoring whatever that was,
I have a fun, side quest-y mini-project for singularity.
I am supposed to give a 5 minute presentation
and then a 3 minute demo on singularity.
So, I want to do the whole presentation in singularity and then be like:
"Aha! You thought this was a boring slideshow app,
but it was singularity this whole time!"

Since this is due kind of soon,
I'd probably make the slides in google slides and export it as a PDF.
To display a PDF, I'd first add PNG/JPG support to Singularity,
then I'd just use
https://github.com/pdf-rs/pdf_render/blob/master/examples/pdf2image/src/main.rs
to convert each PDF page to an image.

Oh, and another thing I want to note is a really cool
digital interface analysis YouTuber I came across named
[InterfaceStudies](https://www.youtube.com/@interfacestudies/videos).
Some videos:
- [Verb vs Noun order](https://www.youtube.com/watch?v=jP5PQ8ix7JE&pp=2Ab5Cw%3D%3D)
- [Pie menus](https://www.youtube.com/watch?v=6uTSwJ3uqEg&pp=2AYC) (vs linear menus)

2026-01-31 6:37PM

I've been chipping away at the treeview little-by-little over the past few days
and didn't even log because I thought it would be easy and simple.

But Darnwin damn it, I currently feel like recursive treeview
is the dumbest freeeeeaking idea I've ever created.

I'm telling myself to stay calm.

Did I even think this idea through
before committing to it-before ditching a working prototype?

But secretly, I think I have stumbled onto an even better architecture.
(*Reader who has the power of foresight, or perhaps just basic common sense and pattern recognition facepalms as I propose an entirely new organizational paradigm.*)

Let me propose the specific version first then generalize.

The idea is called dimension-tree
(or maybe: world-tree, order, axis, level, layer).

For the specific example, lets say dim zero is project,
dim one is applet, and dim two is applet elements
(like if the applet was a markdown reader, it could be the markdown elements).
Then there would be a project tree,
each project holds an applet tree,
each applet holds an element tree.

Dim is short for dimension btw.
Btw is short for by the way by the way.

(I get excited and start talking about extending this idea:)
We could even say dim negative 1 is is task-type
(like coding, music, etc) and so on
but keep in mind that the numbers don't really matter.
If I end up displaying the dim, I'd try to ensure that the lowest dim is 0.
We could go even more even further and say that the layers are not fixed,
and the layer depends on what world this tree is inside
(I think this is similar to dependent types in type theory).

2026-02-02 3:55PM

I don't think the dimension tree actually solves the problem I have with the recursive node.
At least not in the way I want.

Let me try to define a dimension tree that is used slightly differently
from how I described it above, to let it "solve" the recursive node problem.
`DimensionTree<T>` is a tree where a node's value is either
another `DimensionTree<T>` in which the node is a world with a subtree
or a `T` in which case the node is elementary.

<!-- ...

I don't think this is it, gang.
The dimension tree is a generalization of the project tree -> applet tree -> element tree idea,
but I feel like project tree is so solid that I don't need to generalize it.

But, this helped me come to a simple solution to the recursive node thing:
there wasn't a problem with the architecture,
I just needed to think about the root differently. -->

2026-02-03 4:28PM

...I am not sure if this idea is going to solve anything.

Technically, it does solve the problem,
but it just changes/adds so many other things
that the problem is kind of irrelevant to this decision.
In other words, if I were to do the dimension tree,
it would be because I prefer it in general,
not just because it solves the problem.

The problem with the normal tree and recursive nodes
is that since the recursive node's value in the hierarchy
is the main applet's hierarchy value,
the main applet should return `T` (currently just a String) but the general framework
makes it return `Tree<T>`.
A quick solution is just to take the root value of the main applet's output.
There are safer ways like creating a new type of applet that does return `T`.
I also asked myself if it is possible to not have a main applet
and just have children applets that are all equal,
but I don't think that would work.

You know what, I am feeling freaky today,
so I will make the rash decision to start implementing the dimension tree.
I am sure to blame this moment when I run into a problem later,
but that is a problem for future me.

I'll commit what I have now
(what I was working on before I decided to jump ship for `DimensionTree`).

...

> The problem with the normal tree and recursive nodes
> is that since the recursive node's value in the hierarchy
> is the main applet's hierarchy value,
> the main applet should return `T` (currently just a String) but the general framework
> makes it return `Tree<T>`.

Another way of phrasing this is:
in a tree `Tree<T>`, the value is type `T` but children are type `Tree<T>`.
In our recursive node applet, the main "value" applet and children all have the same type.

The `WorldTree<T>` solves this problem,
because it holds either:
`T` itself (base case)
or holds the value of `WorldTree<T>` and children are also `WorldTree<T>`.

...

If this goes to shiz, I can just modify it to be a normal tree again.

Things to figure out:
- Traversal
- Display
  - How to display focus
  - World dimensions:
    - Could have the inner worlds displayed inside (could be crowded but also pretty and elegant, sounds hard to implement)
      - Would support having everything displayed out, only the focused guy displayed out, or allowing user to open and collapse
    - Could have different worlds horizontally split
      - I think mac's file explorer has something like this

I think an elegant way of thinking about this is to think about symbolic tree.
The symbolic view aka treeview is actually kind of like looking at the HTML document tree
(functionally different, but is kind of familiar so bear with me).
Each node in the treeview document tree can draw to the screen.
A difference is that we aren't actually just drawing the focused thing;
we are always asking the root to draw and simply telling it what to focus on.
One could propose a method where we ask the focused applet to draw and the focused applet
could then call its parents when needed,
but I see no benefit of this.

2026-02-06 11:05AM

I am going to try implementing now.

2026-02-07 10:10AM

Here is the interpretation of the treeview
(the name doesn't make as much sense anymore, but I think it is a cool name):
apps have a hierarchy on their own, which could be lazy and dynamic,
but the treeview is simply a symbolic cache of this heirarchy.
(Rn, the symbol is names in string.)

2026-02-10 12:48PM

I don't know my old code for displaying trees is.
I guess I'll have to implement it again.

I will just start out with only displaying the current layer,
and always showing the treeview.
No previewing the select.
No highlighting the focused.

2026-02-16 3:10PM

I am not worried about this right now,
but moving focus should be done by the currently focused app 99% percent of the time,
where they either do it on their own or it calls the parent.

2026-02-19 10:56AM

I am working on implementing focus.
I am actually going to ignore caching for now,
since that is just an optimization.
This means that passive applet will be very slow,
but I'd rather have a slow demo than a non-existent one.

Also, somethning completely unrelated:
I was thinking of an efficient way to store a tree's structure
without caring about the values.
The best algorithm I've thought of so far is with a bitmap.
To create the bitmap, we pretend like we are traversing through the tree in
more or less pre-order (but you also print on the way back up),
and append 0 to the bitmap when we go down and append 1 when we go up.

An interesting thing about this algorithm is that it stores the order of children.
If the order of children matters, this is good.
If it doesn't, then there is redundancy.
There is also redundancy because there can never be more 1s than 0s
and at the end, the 1 and 0 counts must equal.
This is the parenthesis stacks rules,
so if there is an algorithm to compress the parenthesis stacks,
we could apply it to this bitmap.

In general, representing some structure as data should avoid
redundancy by ensuring:
two different data representations lead to same structure
and all data representations have a structure.
A possibly slow but efficient way of ensuring efficiency
is simply to order all the structures and storing the unsigned integer order as the data.

...

Indexing the world tree and modifying paths makes me feel mad.
It could also be the fact my hair makes me look like a stereotypical conspiracy theorist.
I can't help with the hair,
but I will draw crude ascii art to help with the world tree confusion.
Look at the following world trees where a node's value (in parentheses)
is the path it takes to get there.
I will make it more complex as you go downwards:

```rust
// Just a value (level 0)
( [] )

// Structure of a normal tree (level 1)
( [[]] )
|
|--- ( [[0]] )
|
|--- ( [[1]] )
|
|--- ( [[2]] )
         |
         |--- ( [[2, 0]] )
         |
         |--- ( [[2, 1]] )

// Level 2: tree in a tree
// Now imagine that the above tree is still the outermost tree,
// but replace its value with the following tree:
...
( [[2, 1], []] )
|
|--- ( [[2, 1], [0]] )
|           |
|           |--- ( [[2, 1], [0, 0]] )
|           |
|           |--- ( [[2, 1], [0, 1]] )
|
|--- ( [[2, 1], [1]] )
            |
            |--- ( [[2, 1], [1, 0]] )
            |
            |--- ( [[2, 1], [1, 1]] )
```

2026-02-20 5:13PM

I could also make a new data struct like `FocusedWorldTree`
that is a world tree but every node also holds its local focus.

2026-02-21 10:43AM

I hit a recursion limit while compiling for some reason.
I will commit now to leave a record of this weird error.

```rust
error: reached the recursion limit while instantiating `RecursiveTreeNode::<WorldTree<...>>::append_to_string_with_prefix::<...>`
  --> singularity_common/src/utils/tree/recursive_tree.rs:94:13
   |
94 | /             child
95 | |                 .append_to_string_with_prefix(s, &value_stringizer, &child_prefix)
   | |__________________________________________________________________________________^
   |
note: `RecursiveTreeNode::<T>::append_to_string_with_prefix` defined here
  --> singularity_common/src/utils/tree/recursive_tree.rs:82:5
   |
82 | /     fn append_to_string_with_prefix(
83 | |         &self,
84 | |         s: &mut String,
85 | |         value_stringizer: impl Fn(&T) -> String,
86 | |         prefix: &str,
87 | |     ) -> std::fmt::Result {
   | |_________________________^
   = note: the full name for the type has been written to '/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/singularity_common-2cff6ef231fd4809.long-type-8030479317095937628.txt'
   = note: consider using `--verbose` to print the full type name to the console

warning: `singularity_common` (lib) generated 8 warnings
error: could not compile `singularity_common` (lib) due to 1 previous error; 8 warnings emitted

Caused by:
  process didn't exit successfully: `/home/mathkimchi/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc --crate-name singularity_common --edition=2024 singularity_common/src/lib.rs --error-format=json --json=diagnostic-rendered-ansi,artifacts,future-incompat --diagnostic-width=147 --crate-type lib --emit=dep-info,metadata,link -C embed-bitcode=no -C debuginfo=2 --check-cfg 'cfg(docsrs,test)' --check-cfg 'cfg(feature, values())' -C metadata=6576969d03177834 -C extra-filename=-2cff6ef231fd4809 --out-dir /home/mathkimchi/Documents/GitHub/singularity/target/debug/deps -C incremental=/home/mathkimchi/Documents/GitHub/singularity/target/debug/incremental -L dependency=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps --extern paste=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/libpaste-d0c2464c2e21dc95.so --extern serde=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/libserde-6cbd7ea2176590a6.rmeta --extern serde_json=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/libserde_json-82a21528811a9091.rmeta --extern singularity_macros=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/libsingularity_macros-45dc683801d26304.so --extern singularity_ui=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/libsingularity_ui-1da54f7a021d6d83.rmeta --extern uuid=/home/mathkimchi/Documents/GitHub/singularity/target/debug/deps/libuuid-5db6acd5583582aa.rmeta -L native=/nix/store/p1ackxjqznm2q5dl3r178wp487q86jig-freetype-2.13.2/lib -L native=/nix/store/8vi4i41i9w86i3hc925lb2n6r0css4ih-fontconfig-2.15.0-lib/lib -L native=/nix/store/p1ackxjqznm2q5dl3r178wp487q86jig-freetype-2.13.2/lib -L native=/nix/store/4iyki6wsawj3qyisw3yqqam6x7w50had-libxkbcommon-1.7.0/lib` (exit status: 1)
```

...

Hmm, viewing the type written in the file shows:

`RecursiveTreeNode::<WorldTree<std::string::String>>::append_to_string_with_prefix::<&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&{closure@singularity_common/src/utils/tree/world_tree.rs:65:54: 65:60}>`

so that is probably the problem.

...

I just had to take in `&impl Fn(...)` instead of taking in `f: impl Fn(...)` and calling
`&f` recursively.

...

2026-02-21 12:41PM

Noice!
After filling out all the `todo`'s and squashing all the compile-time errors,
it looks like this is working on the surface-level.
Let me commit first and then check for bugs.

(I already know there is an error with my tree display logic.)

...

Hmmm...
I thought making BasicApplet ask to spawn RecursiveNodeApplet(BasicApplet)
would fix the tree hierarchy,
but when spawning from a child, the treeview doesn't change,
so I assume that it is creating it as an inner world for some reason.

I think I need to test out tree printing first.

...

I will use the algorithm from:
https://andrewlock.net/creating-an-ascii-art-tree-in-csharp/
to draw the tree.

...

Okay, so now the normal tree display is working,
so I can be pretty sure that the bug is in the logic of the structure,
not in the display.

I'm going to display all the worlds as I traverse down worlds in the focus path.

...

I wrote the algorithm for the display,
but the focus management itself doesn't work.

I am going to define these temporary shortcuts:

- Q - out world
- E - in world
- A - to parent
- D - to child
- W - up sibling (can set this to clamp, wrap around, or go to parent like dfs prev)
- S - down sibling (same thing)
- 0..9 - to n-th child

Alt is annoying to me because I remapped it on my laptop keyboard.

This is going to be tedious,
but let me walk through the big picture.
It would help to look at an example world tree.

```rust
Level 2 outer:
([[]])
  ├─([[0]])
  ├─([[1]])
  ├─([[2]])
  │   ├─([[2, 0]])
  │   └─([[2, 1]]) <- (contains world)
  └─([[3]])
      ├─([[3, 0]])
      └─([[3, 1]])

Level 2 indexed at [[2, 1]]:
([[2, 1], []])
  ├─([[2, 1], [0]])
  │   ├─([[2, 1], [0, 0]])
  │   └─([[2, 1], [0, 1]])
  └─([[2, 1], [1]])
      ├─([[2, 1], [1, 0]])
      └─([[2, 1], [1, 1]])
```

Suppose we are focused at `([[2, 1], [1]])` and for now,
just pretend like it doesn't also contain a world.

I am currently thinking that the world tree doesn't make sense.
Let me draw out an example use-case on a whiteboard and walk through it.
For this, I will use universally consistent layers:
layer 0 - project,
layer 1 - applet,
layer 2 - items.

(Universally consistent is somewhat like
having matrices instead of variable-sized arrays of arrays.
A matrix has universally consistent sub-array size.)

2026-02-22 6:40PM

I think for focus,
each recursive node applet can store three possibilities for focus:
1. focus on self (or Focusing)
2. focus on inner
3. focus on child

The difference between focus on self and focus on inner is that focus on self is kind of like the transition state
and the focus on inner means that input is actually being passed.

...

2026-02-22 9:02PM

I talked to @MrPoWasTaken about the world tree,
and the conclusion was that I should continue with the world tree for now.
The reasoning was that I should either fully commit to different layers (do world tree)
or just not do it at all (normal tree),
instead of doing hard-coded layers (like a project tree, project being an applet tree, and applet having elements tree)
since fully generalized layer seems to not have downside to hard-coded layers.
And since I prefer hard-coded layers over no layers,
the conclusion is that I should at least try generalized layers for the MVP.

I decided to put the main applet of recursive node applet in a seperate field
instead of putting it in the vec of all applets.
So, I tried using a placeholder applet for instantiation as a botch
but now it looks like a less botched solution might actually be easier,
so I will do that.
I am just going to create a seperate struct that doesn't have the main applet vs one that does.
I will commit now just to save the botch work (that doesn't work).

...

2026-02-24 11:14PM

I suspect that my traversal code could be working,
but I really can't tell what's happening because I can't see what is focused.

So, I guess that's my next task.

2026-02-25 2:43PM

I am kind of printing the things,
but there is a weird bug.
Take the following move sequence:
into, add child, out, 0-th child.
Allegedly, your path is `[[0]]` and your input isn't sent to the child inner
(try typing Hi or adding subchild or quitting, nothing happens).
Then, go into again.
It prints that your focused path is still `[[0]]`,
and the focus indicators on the treeview don't change.
But, clearly something happened because it lets you interact
with the inner app.

I think that the path should be `[[0], []]`.
So, I guess I'll have to bug find.

2026-02-27 1:54PM

Wait, what the heck?

I got it working, but I don't know why.

I had to make it so the text applet returns `[[]]`
and focusing state also returns `[[]]`.
(Making focusing state return `[[]]` was the thing I really missed.)
I also think I am technically going into a value,
but I don't care.

I can believe it because I haven't thought about it too much,
but it feels unintuitive.
I'm not going to question it though because it seems to be working.
Maybe I bug-test it later, but I've been really frustrated about this
so I don't care.

...

Now that the bug is fixed,
I'm just going to add more traversal logic and operations.

2026-02-28 5:20PM

These are the bindings, as a reminder:

- Q - out world
- E - in world
- W - up sibling (can set this to clamp, wrap around, or go to parent like dfs prev)
- S - down sibling (same thing)
- A - to parent
- D - to child (either 0th or previously selected child)
- 0..9 - to n-th child

As a side-task,
I should do warnings with log or tracing crates.
Before that, I should first merge this branch to master.

In terms of just the framework, I'd say that I actually do have a MVP right now.
But for a presentable MVP, I need to add more basic features as well as apps.

## [#24 Towards an MVP](https://github.com/mathkimchi/singularity/issues/24)

2026-02-28 5:34PM

So I merged to the dev branch,
and I am going to just work in dev for the forseeable future,
until I have an MVP, need to make a big risky change, or work with someone else.
My reasoning for this is similar to how the semantic versioning system
`x.y.z` only allows breaking changes when `x` changes,
but when x=0 (pre-alpha), then its a free-for-all
because it's so primitive that we just assume that there are going to be
so many breaking changes.

I am not feeling eager to write about my general plans-I think
I know what to do and I'd rather do the plan right now than talk about
it-but I guess I should just mention it so I don't forget.
(Btw, I just learned that people actually use em-dashes in real life.
And another btw, I haven't used LLM chatbots or code assistants for a few months.)

I'll put the checklist in a seperate issue.

For me, a time manager was a big thing I desired from singularity.
However, it is not critical for an MVP, so I'll do it after.

Ok, I made the issue and also have a subissue
([#26](https://github.com/mathkimchi/singularity/issues/26))
for traversal,
so I'll commit the plans now and get working on that.

2026-03-02 8:18AM

I am consolidating all the movement logic of the recursive node applet
into one function,
but the problem is that a traversal operation means different things
based on who is calling it aka the current focus.

This is technically redundant but I'll just change behavior based
on current focus state
(it is redundant because based on why the event is happening,
we can infer the focus state already).

2026-03-02 9:22AM

I feel like the only thing really left to do now is the layerwise DFS.

I am thinking of replacing the sibling relshifts keybinds (`w` and `s`).

...you know what, I don't need to do this.
I am working towards an MVP
(wow, he said the thing!)
and while extremely scuffed, I think this is fine.
Especially since the point is to create a workspace and not a game or something.

I'll start making basic apps now.

2026-03-02 11:47PM

I thought embedding Alacritty was going to be
easier than I expected for a few minutes.
It's not.
I thought maybe proper colors would fix it,
I couldn't even do proper colors.
I thought maybe fixing input would fix it,
it didn't.

I haven't even figured out how
actually running a terminal app is supposed to work.
I thought it would automatically come with Alacritty.

I am unironically considering just looking into Smithay again.

...

Nope, no.
No, I'm not going to make a Smithay Wayland compositor
until I have the MVP.

2026-03-03 11:59AM

I am going to try to embed servo.

While waiting for Servo to dependency to build,
I was considering new names for Singularity.
I like `sonamu` which means pine tree in Korean.

Anyways, servo's `bitflags` crate dependency seems to be
conflicting with Alacritty's bitflags dependency.

```sh
❯ cargo build
    Updating git repository `https://github.com/servo/servo`
    Updating crates.io index
    Updating git repository `https://github.com/servo/stylo`
error: failed to select a version for `bitflags`.
    ... required by package `libservo v0.0.1 (https://github.com/servo/servo#036cb9a6)`
    ... which satisfies git dependency `libservo` of package `singularity_standard_tabs v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_standard_tabs)`
    ... which satisfies path dependency `singularity_standard_tabs` (locked to 0.1.0) of package `singularity_sde v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_sde)`
versions that meet the requirements `^2.11` are: 2.11.0

all possible versions conflict with previously selected packages.

  previously selected package `bitflags v2.8.0`
    ... which satisfies dependency `bitflags = "^2.4.1"` (locked to 2.8.0) of package `alacritty_terminal v0.25.1`
    ... which satisfies dependency `alacritty_terminal = "^0.25"` (locked to 0.25.1) of package `singularity_standard_tabs v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_standard_tabs)`
    ... which satisfies path dependency `singularity_standard_tabs` (locked to 0.1.0) of package `singularity_sde v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_sde)`

failed to select a version for `bitflags` which could resolve this conflict
```

Since I am temporarily giving up on embedding Alacritty anyways,
I will just comment out everything related to it.

...

```sh
❯ cargo build
    Updating git repository `https://github.com/servo/servo`
    Updating crates.io index
    Updating git repository `https://github.com/servo/stylo`
error: failed to select a version for `bitflags`.
    ... required by package `libservo v0.0.1 (https://github.com/servo/servo#036cb9a6)`
    ... which satisfies git dependency `libservo` of package `singularity_standard_tabs v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_standard_tabs)`
    ... which satisfies path dependency `singularity_standard_tabs` (locked to 0.1.0) of package `singularity_sde v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_sde)`
versions that meet the requirements `^2.11` are: 2.11.0

all possible versions conflict with previously selected packages.

  previously selected package `bitflags v2.8.0`
    ... which satisfies dependency `bitflags = "^2.4"` (locked to 2.8.0) of package `smithay-client-toolkit v0.19.2`
    ... which satisfies dependency `smithay-client-toolkit = "^0.19"` (locked to 0.19.2) of package `singularity_ui v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_ui)`
    ... which satisfies path dependency `singularity_ui` (locked to 0.1.0) of package `dylib_math_game v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_standard_tabs/dylib_math_game)`

failed to select a version for `bitflags` which could resolve this conflict
```

Hmm... it seems that now it conflicts with smithay client toolkit's bitflags.
After actually reading the error,
it seems like servo wants a later version of bitflags than the other packages.
I'll see if `cargo clean` changes anything.

...

Nope, still conflicts with smithay.
Maybe I should update smithay from `^0.19` to `^0.20`?

...

```sh
❯ cargo build
    Updating crates.io index
    Updating git repository `https://github.com/servo/servo`
    Updating git repository `https://github.com/servo/stylo`
error: failed to select a version for `yeslogic-fontconfig-sys`.
    ... required by package `font-kit v0.11.0`
    ... which satisfies dependency `font-kit = "^0.11.0"` of package `singularity_ui v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_ui)`
    ... which satisfies path dependency `singularity_ui` (locked to 0.1.0) of package `dylib_math_game v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_standard_tabs/dylib_math_game)`
versions that meet the requirements `^3.0.0` are: 3.2.0, 3.1.0, 3.0.1, 3.0.0

package `yeslogic-fontconfig-sys` links to the native library `fontconfig`, but it conflicts with a previous package which links to `fontconfig` as well:
package `yeslogic-fontconfig-sys v6.0.0`
    ... which satisfies dependency `fontconfig_sys = "^6"` of package `servo-fonts v0.0.1 (https://github.com/servo/servo#036cb9a6)`
    ... which satisfies git dependency `fonts` of package `libservo v0.0.1 (https://github.com/servo/servo#036cb9a6)`
    ... which satisfies git dependency `libservo` of package `singularity_standard_tabs v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_standard_tabs)`
    ... which satisfies path dependency `singularity_standard_tabs` (locked to 0.1.0) of package `singularity_sde v0.1.0 (/home/mathkimchi/Documents/GitHub/singularity/singularity_sde)`
Only one package in the dependency graph may specify the same links value. This helps ensure that only one copy of a native library is linked in the final binary. Try to adjust your dependencies so that only one package uses the `links = "fontconfig"` value. For more information, see https://doc.rust-lang.org/cargo/reference/resolver.html#links.

failed to select a version for `yeslogic-fontconfig-sys` which could resolve this conflict
```

So bitflags isn't the error anymore, but something with fonts is.
I'll just update font_kit from `^0.11.0` to `^0.14.3`.

...

12:32PM

I've been fighting with random build commands written in c.
I am trying to add nix packages to make it work.
`tikv-jemalloc-sys` just keeps on giving me more errors.

Other people https://github.com/NixOS/nixpkgs/issues/370494
seem to have had the same problem.

You know what, instead of thinking about it,
I'm just going to throw all the C related packages into my flake.nix
and hope one of them solves the problem.

...

I'm fully cleaning my NixOS system because flake is not recognizing llvm 22 for some reason.
I thought flake was supposed to prevent those issues from happening, but oh well...

Since fonts are being weird, I also ran `rm -r ~/.cache/fontconfig`.

...

2026-03-03 11:44PM

Still no luck,
I am looking at the specific failing crates and trying to search up their dependencies
or if someone who uses nixos+rust+that crate has made an issue yet.

For example, fontsan-woff2 is failing.
It has no documentation but its dependant [`fontsan`](https://crates.io/crates/fontsan) does,
and has a list of dependancies:
- ots
- lz4
- brotli
- woff2

so I will try to get those.

...

Still breaks, I'll try running build with sudo.
I need to first run `sudo rustup default stable` because ai guess rust was only installed on my user.

...

It was taking over 5 minutes just to get servo again.
I committed in the meanwhile because I was already pretty sure it wouldn't fix the problem.

Apparently servo uses some alternative to `cargo` called `mach`
and I have to use `mach`: https://book.servo.org/contributing/editor-setup.html#nixos.

6 years ago (was just 2020, yikes!),
there was this issue: https://github.com/servo/servo/issues/27613 saying they couldn't build servo either.
The error seemed to be with dependencies and the "fix" was hardly a fix.

I'm going to try using the new release [0.0.5](https://github.com/servo/servo/tree/release/v0.0.5)
from 4 days ago.

...

Okay, I am giving up on embedding servo.
I can't embed it if I can't even build it.

The annoying thing is that running it on nixos was really easy,
so I thought this would be easy as well.

But I don't want to force singularity to rely on `mach` until I do my own research on it,
which I do not have time for.
So for now, I will give up on embedding Singularity.

...

I will undo the mess I created trying to get servo to work.

...

As I update my cargo dependencies, I can run `cargo update --verbose`
to see what needs updating.

2026-03-04 11:48AM

As I said in [this comment](https://github.com/mathkimchi/singularity/issues/27#issuecomment-3995439265),
I am just going to work on the command hub.

Technically, command hub is a standard app so it should go under singularity_standard_tabs,
but it is also a tool so I am justifying its place in singularity sttk.

...

12:40PM

I locked in for 50 minutes,
and I am pretty happy.

I just have addition working right now,
but it's pretty cool and it was easier than I thought.

I'm going to have to actively suppress my urges to make my own scripting language.

...

1:00PM

Making this run shell commands had a very minor hiccup with `spawn` vs `output`,
but it was also surprisingly easy.

It is very bad though, since it just waits until the program is finished
instead of running it async.

If I run `$ sleep 10` twice in a row without waiting,
it sleeps for 10 seconds, then updates the ui,
then sleeps for another 10 seconds.
This makes sense.
The holder for this is probably waiting on a mutex somewhere.

...

2026-03-05

I added command hub with very jank "parsing" (just split tokens).

I don't 

...

2026-03-07 6:11PM

Erhm, I kinda lost steam in the previous entry and didn't finish my sentence.

I've been focusing on other things like finals, composing, and beginning to exercise again.
But now, it's break so I'm still going to exercise,
but there's no more finals.
Plus, most my friends are traveling or going back home or visiting schools
so I won't be wasting time with all that
"making memories with friends before I graduate" business (sarchasm).

In all seriousness, I've recently cut off the final unfulfilling time-wasters in my life.
Namely, YouTube and Instagram.
So, I'll try to do more productive things
(like Singularity, music, maybe coding streams?) to overcome any urges I might get to
suddenly start doomscrolling or something.

I think I was going to say that I'll consider the Command Hub done for now,
in the sense that I will move on.
But, it isn't even done for the MVP yet, because I'll need to add actual children.

Now, I'll work on a text editor, which should be really easy since I've done it before.

...

Before that, I'll make "standard applet" which implements
the standard applet shortcuts like add child and traversal.

...

Actually, instead of making a wrapper for the applet,
I can just make a `handle_standard_keybinds` function.
Later on, I want to do something more generic,
but again, I am working "Towards an MVP" (wow, he said the thing again!).

I feel like a Java OOP dev right now,
becase I thought to do something that could so easily be done via a function
with inheritance instead.
I guess I got tunnel vision from making all the other wrappers.

...

7:28PM

So, I made the editor.
It was pretty easy, since I already made a text editor in the old framework,
and since I also made the text box
(lets you write text, but doesn't save to a file) in the current framework.

Now, the question is how to run it.

I have two general options that I am considering:
- some system of registering commands or at least applets (which I'll do eventually)
  - Could either be global (simpler) or do something similar to environments similar to env logger where applets inherit parent's environment (I'll probably need to have the env manually passed down instead of implicitly, which is how I think env logger does it)
- A temporary solution: just move standard commands into sttk and hardcode the applets

While I understand that I want to get an MVP asap, this is a pretty crucial feature,
and I don't know if I can say Command Hub can be done until this is done.

I'll give myself today and tomorrow to think of a command and env system.
I still won't have an actual language for the MVP though.

It seems like env_logger just uses shell env.
I thought they did something cooler with like RAII,
but it seems they just have something that sets the `RUST_LOG` variable on init
and unsets it on drop.

That's lame.
I never liked env variables.

If I wrote my own OS and language for that OS,
I would make the environment an object that is passed from the parent to child.
Or maybe I'd have a shorthand (special syntax) for passing the env automatically,
but I don't like the standard way that kind of hides it
unless you actively are thinking about it.

2026-03-08 5:07PM

The bare information I want to store in the Env is:
- commands
- list of applets

...

I went on a walk (I was going somewhere;
I wasn't walking *just* to think about Singularity),
and I realized that the "Env" system doesn't need to be some special thing:
I can just make it a part of the hook.

Eventually, I do want to bring back the abstract packets,
but for the MVP, I just need to add the following operations to the nodular hook
(maybe I should make another struct called the environmental hook or something,
which would be the proper "single responsibility" way of doing things,
but there is no reason to do that right now,
especially since I'll abstract everything later):
- `register_command(&self, name: String, command: Command)`
- `get_commands(&self) -> Map<String, Command>`
- `find_command(&self, name: String) -> Option<Command>`
- `register_applet_spawner(&self, name: String, applet_spawner: AppletSpawner)`
- `get_applet_spawners(&self) -> Map<String, AppletSpawner>`
- `find_applet_spawner(&self, name: String) -> Option<AppletSpawner>`

Now, there's a lot of repetition so I'll abstract
that with a `Registration<T>` class later,
but that's for a later me to think about.

In the list of operations I just wrote down,
you can see two new types: `Command` and `AppletSpawner`.
I might need to rename `Command` to something else like
`SCommand` (for singularity/sonamu command).
These will be clonable and runnable.

For now, I'll just say these take a list of strings as arguments when running,
like in shell.

By the way, it is 2026-03-09 12:02PM right now,
so I'm 12 hours past my deadline, but I went to NY yesterday
and when I got back I was texting so I coudn't work on brainstorming.

2026-03-11 5:21PM

(I've been busy working on music and learning Topology.)

Command registry isn't necessary for the MVP,
so I'll only do the spawnable applet registry right now.

I just realized that these are the first hooks that return something.
I don't think that will cause any complications.

The naming standard for the applets will be snake case,
like most things in rust.

6:40PM

Wow, it works!

I shouldn't be this surprised that I know how to code,
but it's very satisfying.

I can really see Singularity coming together now.

It is like really really slow and the fact that I messed up the saving shortcut
(I was matching for a capital `S` even though the command had no shift).

Next, I want to make a presentation viewer.

...

8:46PM

Hmmm...

~~I thought I would wait until the Linux MVP to port singularity to multiplatform.~~

Scratch that, I'm going to follow the plan.
I really need to keep myself focused if I actually want to see singularity finished.

Okay, but the speed is really a problem, even for an MVP.
I am going to try moving everything from `raqote` to `pathfinder`.
I already use `fontkit`, which uses `pathfinder` since both are made by the Servo project.

...

2026-03-12 8:24PM

I got not so favorable news from CMU and Caltech...
I feel like just doing something fun,
so I am going to play around with wgpu.

Apparently I can use the `mktemp -d` command to make a temporary directory
which I will delete later.
I am going to clone [glyphon](https://github.com/grovesNL/glyphon)
(which is a crate for writing text with wgpu)
and try to run their hello world.

I can then run `nix develop ~/Documents/GitHub/singularity`
to use the settings from singularity in the temporary directory.

Then, I can run their example with `cargo run --example hello-world`.
I get the NoWaylandLib error,
which I remember I had a long time ago.
[This is the fix](https://github.com/iced-rs/iced/issues/2385) from the internet.
I am technically bloating Singularity's flake.nix by doing this,
but I'm going to add the dependencies here.

...

I actually needed the `LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";` line which I commented
when I got destroyed trying to get servo to run.
I now get the error: `Result::unwrap()` on an `Err` value: BadDisplay
which seems to originate from wgpu.

[This issue](https://github.com/gfx-rs/wgpu/issues/5505)
is the same, I think,
but the answer seemed to be to rewrite the code to avoid errors.

I'm not going to rewrite the glyphon example code,
because I don't even understand it.

I'm going to try to see if the `learn-wgpu` example works and if it doesn't work either,
then `wgpu` isn't for me.

...

12:39AM

Haaaaa...

Sometimes, I really ~~hate~~ am disappointed by NixOS lacking compatibility with many small projects.

Before I got on this wgpu rabbit hole, I *was* writing actual code
to use pathfinder or some other gpu-based alternative to raqote.
I have like 30 errors, but I'm just going to commit.

...

2026-03-13 12:56PM

Looking at [this thread](https://github.com/gfx-rs/wgpu-rs/issues/332)
about wgpu not working on NixOS,
their fix is to add a `build.rs`.
The person who asked the question also mentioned using [NixGL](https://github.com/nix-community/nixGL)
which means if I had to run the command `program` that uses OpenGL or Vulkan,
I can run `nixGL program` or `nixVulkan program`.

I'm just going to copy the [glyphon hello world example]() to singularity_ui's examples,
because it's annoying to work on a seperate directory.

...

I can't get the nixgl overlay to work and I am not going to learn Nix right now,
but I have a slightly jank method: just run `nix run --impure github:nix-community/nixGL -- program`.

So, if I run `nix run --impure github:nix-community/nixGL -- cargo run --example glyphon_hello_world`,
it actually works.

I don't know how liscenses work,
so I'm just going to delete the example before I make this commit.

...

I cloned the wayland_backend into winit_backend and then used the snippets from the glyphon thing
to make it use winit and glyphon and wgpu.
Then I spent a bunch of time getting rid of compile time errors.

But when I run it, it tells me: "Initializing the event loop outside of the main thread is a significant cross-platform compatibility hazard. If you absolutely need to create an EventLoop on a different thread, you can use the `EventLoopBuilderExtX11::any_thread` or `EventLoopBuilderExtWayland::any_thread` functions."

...

Now I have to actually draw the UI.

2026-03-14 4:31PM

Bro...

What kind of "pure-Rust graphics API" requires `wgsl`...?
(This is giving Troy Barnes "Market price? What market are the shopping at?"
from Community.)

Well, I asked @glolichen if I should learn Wgsl just for this,
and he said yes.
So I guess I have to now.

The only thing I really care about is how data can be sent.

...

I reproduced [lesson 3](https://sotrh.github.io/learn-wgpu/beginner/tutorial3-pipeline/#writing-the-shaders)
which is a pipeline that outputs a triangle.

I'll start [Lesson 4: Buffers and Indices](https://sotrh.github.io/learn-wgpu/beginner/tutorial4-buffer/#so-what-do-i-do-with-it).
Buffers seem to be the method of sending data to the gpu.

2026-03-16 2:57PM

The wgpu lesson 4 is working to draw a pentagon out of triangles.

I am going to turn the logic from [Zed's sdf GPU blog](https://zed.dev/blog/videogame)
into WGSL and WGPU.
(They use metal in their examples, I think.)

...

As I wrote more and more WGSL, I thought I was going crazy because it is so similar to rust.
I looked it up, and yes, their syntax is primarily based on rust.
I don't feel so stupid for mistaking the WGSL code for rust now.
(Before starting working with wgpu, I skimmed through the tutorials and thought their wgsl
code was just rust with special macros/annotations.
That's why I was so surprised when I saw they required a shader language.
But I'm not as sad anymore, because it's actually pretty fun to learn new things
and I want to be able to say I can code a shader language.)

I've been just using the code from the tutorial and reading just enough to make things work,
but maybe I should just sit down, take it slowly, because I'm still young
(sorry, I've been listening to Father and Son a lot and I that line just played).
As I was going to say, I am going to actually go back and read lesson 3 and 4.

Those who look above with questions fail to see the answer in front.

Here are my lesson 3 notes:

Vertex shader manipulate points in 3d or 2d space to make shapes.
I think vertex shader is run once per vertex.

Fragment shader turns fragments (that somehow come from vertices)
into color.
Fragment shader is run at least once per pixel.

The basic graphics pipeline is the vertex shader output going into fragment shader.
(But idk how this works bc I thought vertex shader outputted a modified vertex and frag shader inputted a fragment.)
Observationally, I am assuming they make triangles and use lerp?
That's what happened when I used vertices of different colors.

The `@builtin(position)` annotation on a field says that field is the clip position.
I guess the clip position is the vertex's transformed position after applying perspective position,
and before a vertex is passed to frag shader, it is used to see if it is in the camera's view.
I'll only be working in 2d, so I wonder if there is a way to ignore this.
I previously looked for a way to completely ignore vertex shaders
(since they work with triangles which make sense in 3d but I'd prefer just directly working with AABBs),
but I think I need to just have a trivial vertex shader and have two right triangles that span the whole screen.
Since I'm just working in 2d, the important thing about the `@builtin(position)` is that the x and y
are in pixel space.

...the rest of lesson 3 was mostly just mentioning extra options
but just saying it will be explained later.

Lesson 4 notes:

As I said earlier, buffers are how we pass data from the cpu to the vertex shader.

The `VertexBufferLayout` is our description of the data that will be passed to the GPU.
The `step_mode` of `wgpu::VertexStepMode::Vertex` or `wgpu::VertexStepMode::Instance`
does actually seem important, because the Zed tutorial actually mentions
using `instanced rendering to draw multiple rectangles to the screen in a single draw call`.
(Tutorial 7 in instancing.)
The attributes are like the fields of the vertex shader input.
The `shader_location` of a vertex attribute should be the same as the corresponding
`@location(x)` as the wgsl code.

Then we actually set the buffer with `render_pass.set_vertex_buffer`.

The index buffer lets us specify the index of some vertex's data instead of having to
pass all its data each time.

I diverged (slightly) from the tutorial to include
`@builtin(vertex_index) index: u32,` in the vertex shader input.
I don't actually use it, but since it compiles and runs,
I think it works.
The tutorial uses `u16` as the index format,
but u16 actually isn't a wgsl primitive (idk how the u16 index works)
so I just made the index `u32` on both the rust and wgsl sides.

I also looked at [another tutorial](https://webgpufundamentals.org/)
that a [redditor really liked](https://www.reddit.com/r/rust_gamedev/comments/18o5wa1/the_best_wgpu_tutorial_ive_found_its_relatively/).
It is just wgsl in general though and not wgpu (it uses js to run it),
so I am not going to read too much of it,
but I found a [neat trick](https://webgpufundamentals.org/webgpu/lessons/webgpu-large-triangle-to-cover-clip-space.html)
to use one big triangle instead of two triangles to render the whole screen rectangle.
It is an extremely minor speed improvement, but it is pretty cool.

I'm going to commit now and then look into
[Tutorial 7: Instancing](https://sotrh.github.io/learn-wgpu/beginner/tutorial7-instancing/#the-instance-buffer).

2026-03-17 4:32PM

Actually, I'm going to get my port of Zed's rounded rectangle sdf metal logic
working for one rectangle instance first.

Let me also just say that swizzling (being like v.xy or v.wwxz or ... to quickly transform it) is my goat.
I actually do worry about type safety and stuff with it,
but the name swizzling is so good I don't even care.
It makes rearranging a vector's components sound like a cool skateboarding trick.

Wow, it works.
I did screw up the triangle order,
but other than that, it works pretty well.
Pretty proud of getting this to work (even though the logic wasn't mine).

...

7:30PM

Hmmm...

I *am* able to pass the rectangle data through instance buffers,
but the problem is that since I am drawing to the whole region each time,
each rectangle erases the previous rectangle.

I think that's because I was using an alpha mode of Opaque,
meaning that alpha is simply ignored.

I set my thing to pre-multiplied, and apparently there are other ways of doing transparency
like post-multiplied,
but since I'll only be using alpha of 1 or 0, I don't think they should matter.
(I'm just using alpha as a way to say "ignore this".)

...

Erhm, I was kinda hoping it would work first try,
but it is actually worse than I imagined.

Firstly, it's still just rendering one rectangle.
The transparency kinda shows what *was* behind, but there's an emphasis on *was*.
If the thing behind updates, it doesn't show up through transparency.
And if you move the window, it starts trying to draw what it was before,
so you get this glitchy recursive mess.

I'm going to commit just so people can see what I'm talking about.

I think the fix might be with blending mode?

...

7:42PM

For each of color vs alpha, I can set their blend component to OVER or REPLACE.

It was previously both set to replace.

With Color: OVER and Alpha: REPLACE,
I *do* get two rectangles and it shows a cyan background
(I think the background was supposed to be half-transparent cyan but it didn't show up before
bc bug),
but when you drag, it actually becomes white (I think the transparent cyan keeps stacking until all channels are maxed).

I'm pretty sure the correct setting is going to be
setting both to OVER,
but I'll try the other remaining option first for dramatic purposes:
Color: REPLACE and Alpha: OVER.
It just fully ignores transparency.
Just one rectangle, black background.
That's anti-climactic, I'm glad I saved the more hopeful one for last.

Now with both OVER:

This, is why I'm the goat, this is why we science our computers!

I make it sound like something really cool,
but it just works.
I mean, its cool that it does but it's lowkey embarrassing how excited I got.

The two rectangles show and the background is actually just cyan,
not a half-transparent one.
I think the background was always set to an opaque cyan.
I'm gonna commit now and see where I define the default background.

...

7:59PM

I tried making the background half transparent,
and the artifacts came back.

I'm just going to keep the background opaque (I never liked transparent terminals and stuff anyways).

...

I need to now draw different shapes as well,
meaning I probably need different types of instances.

Google says that the simple way of dealing with multiple types of instances is with multiple passes,
but I don't think that will work for me because
the instances aren't ordered by type
(if I do the rectangle pass then the text pass, then even text that's supposed to be hidden
will end up above the rectangles).

I mean, I could draw each shape one at a time,
but there's gotta be a better way.

One idea I have is to use uniform buffers
(which I guess are like constants)
to have a list of each shape and its data.

Then, each instance data would just be the index indicating which shape type
as well as the specific shape's index on the shape-type's list.

...

2026-03-18 5:50PM

I might actually end up doing a draw pass for each UI element,
since it will allow me to generalize.
I don't know if I should just make generalization done through compositing
or if I should let them directly make their own UI elements.
Theoretically, allowing custom UI elements will have the faster best case scenario.
But, compositing will be simpler and prevent one really slow draw from slowing everyone else down.

2026-03-19 12:08PM

As I start thinking about other element types,
I'm going to take notes on how [Clay](https://github.com/nicbarker/clay),
a UI library in C, does layout.

...

As I remember (I watched this video a year ago when it came out),
it's a good video.

But the text really seems to not be elegant.

I am also tempted to just force apps to make do with whatever sizes their parents give them,
because I'd eventually like to make a Zooming UI.

I always imagined the GUI would be the difficult part and everything else would be simple about UI,
but coming up with a good, not too restrictive abstraction for a UI element is hard.
I have to consider layout (sizing), rendering, and updating.

I am also curious about Vector graphics for UI:
https://en.wikipedia.org/wiki/Vector-based_graphical_user_interface
which I came across on Wikipedia while looking at ZUI.

If performance wasn't a problem,
I think my ideal abstraction for UI elements would be
elements that take in a 2d size and a single point in that space,
and return either a color or a None (meaning transparent).

It would be a more mathematical/geometric way of doing UI,
and it won't be fast and it probably won't even look that good.
But it makes sense to me.

I mean, I guess there's something neat about compositing as well.
Compositing is pretty much what I described but not lazy.

The thing about lazy evaluation (I am speaking in general but with the special context of UI)
is that it is best for minimizing calls,
so in theory it seems like it should be the most efficient.
But in practice, we have techniques like multi-threading
where even if we call a function more often than necessary,
it can take less time than a single-threaded lazy implementation
because we are calling the function while doing other stuff.

...

Hrngh...

GPU is more underwhelming than I thought for UI.

It seems like the main factor in an application being "GPU-accelerated"
just means they draw rectangles and stuff on the GPU,
but I feel like I could just use a rendering crate that uses GPU
and say the exact same thing.

Well, I'm already this far, so I'm just going to just replace the SingularityUI
backend to use GPU and call Singularity "GPU accelerated".

I wonder how compositing works with GPU.
Because compositing is just copying over data from one screen to another,
so if the data to be copied is on the CPU,
what's the point of moving it to the GPU then letting the GPU copy it somewhere else?
I've got to be missing something.
Maybe it uses something similar to textures.
Idk, I didn't actually read the thing about textures.

2026-03-24 11:51AM

Yesternight, I got the singularity ui's borders and rectangles
kinda working on GPU.

I don't need to do SDF for hard square corners,
so I'm just going to rewrite this to ignore the corner radius.

...

It works pretty good.

2026-03-24 9:33PM

I am writing this note today just to remember that my next task is
to set up text with glyphon.

I want to work on it right now, but right now,
my composition for my school choir is the passion project taking all my time,
so I will save the working time for tommorow's computer science block,
which should be 50 minutes.

...

11:15PM

I ended up working on it.
I think I'm almost there, but I think it is currently being lazily drawn,
and the text is being replaced before it could be drawn,
so only the last thing drawn will show up.

That's annoying, but I'll commit now because I've been waking up at 6 every day
to jog/gym with some buddies.

Once I get this bug fixed,
I think I will change how some of the UI primitives are represented
(ie, combine the border and rectangle, as well as just using glyphon's rich text
instead of doing each char manually).
Then, I will be able to say Singularity is up and running on GPU.

Speaking of, I can feel that Singularity is very fast now.

2026-03-26 1:38PM

This [github issue](https://github.com/grovesNL/glyphon/issues/113)
talks about the same issue I had.

Supposedly, as long as you call draw, you should be allowed to change the buffer,
but the person who created the issue said you just need to make multiple renderers.

It's not ideal, but it'll do.

This [wiki link](https://github.com/gfx-rs/wgpu/wiki/Encapsulating-Graphics-Work)
found in the issue goes over general advice on how to make custom renderers.

...

Okay, this is working now!

I mean, I'm a little sad I need to create a new renderer each time, but this is fine.

I'll commit with all the debug stuff
then delete next commit.

Wowie, it's so fast!!!

Ok, I'll commit the ones that don't have the debug boxes and warnings squashed.

2026-03-27 8:32AM

I lowkey was planning on making the previous commit be the one that closes
[Gpu acceleration #28](https://github.com/mathkimchi/singularity/issues/28),
but I forgot to do the GitHub thing,
so I'm going to set up the actual text thing to work with glyphon (or rather cosmic text)'s
rich text and get rid of char grid.
This way, it looks like this was the plan from the beginning.

2026-03-27 7:42PM

Welp, I committed to Cornell!!!

It isn't relevant to Singularity/Sonamu,
but for some reason, it would feel cold not to mention it.

2026-03-29 8:38AM

There remains one giant that looms over me.
A Goliath to my Sonamu.

I am talking about a compositor.

I know, yes.
I know, that I am sounding quite ambitious
and that it seems I am just not trying to get an MVP working,
but I'd rather manage this task sooner rather than later.
I actually think making a wayland compositor is going to be easier
than making a good app or even embedding existing Rust apps
like Servo or Alacritty because Smithay has pretty good documentation
(if my memory serves correctly).

I was going to do my Topology homework
(due at 5pm today and I haven't started it; yikes)
but I suddenly just got really angry with the fact that
I was not using singularity.

So, that's what I'm doing now.

I predict that this task will be monumental,
so I am going to make a new crate just for it.

...

Uhh, getting smithay to compile was actually so easy
that I am embarrassed to think I couldn't get it to compile in the past,
lol.

I mean, maybe its because I already had a lot of dependencies,
but the error messages this time just said the dependencies I needed
but didn't have (just `udev` (libudev) and `seatd` (libseat)).
And then it just worked.

By working, I just mean building.
If I recall correctly, this is what
I spent a few days on in the past and caused me to quit.

Well, I guess I'll commit now.

9:16AM

This might come as a big shocker, but I'm actually reading the documentation now.
This is doubly useful because beyond just wanting to know what I'm writing,
I also want to take inspiration from the Wayland protocol and
the Smithay wrapper to improve the architecture of Sonamu.
(Still procrastinating the name change.)

I'll just write down things I learn from documentation,
a readthrough of [Smallvil](https://github.com/Smithay/smithay/blob/master/smallvil),
and just searching into this devlog.
I think I'm going to start by reading through Smallvil
and looking for new terms in an
[introductory wayland book](https://wayland.freedesktop.org/docs/book/Protocol.html).

Linux term:
A seat is a collection of hardware devices,
like keyboards, pointer (like mouse or touchpad), and monitor.
If you had one big computer and multiple users at the same time,
you'd assign a seat to each user.

Smithay's `DisplayHandle` holds and manages all the clients.
It also manages globals and events and objects.
(Globals seem to be used to agree on what packets have what id.
In the wayland side, they have a registry provided by the server,
which is similar to what Sonamu is already doing.
[Source.](https://git.sr.ht/~sircmpwn/wayland-book/tree/master/item/src/registry.md))

2026-04-19 7:02AM

Ugh...
It feels definitely possible to do all this,
but there is just so much that I don't know
and not enough documentation.

My teacher and friends have all told me to use
an LLM to ask clarifying questions.
To my great shame, I am doing that.
So far, it gets some arguments for specific functions wrong
(which is to be expected since I don't think it learned
the specific types of this kinda niche library,
and because there are also multiple versions of it)
but the conceptual explanations seem to make sense.
(I am mostly believing it because this task is
one hard to do forwards but easy to verify backwards).

For the "Image Primitive" (for as long as I am still doing the primitives),
I am going to use Pixman.

2026-05-04 3:07PM

Smithay changed a lot of things in
[this commit](https://github.com/Smithay/smithay/commit/0d14cd655f6e905ad5110ff0384400da223f2cab)
and I just can't get the events to happen.
(I can compile and run, but nothing happens and most of my debug prints aren't triggered.)

So, I'm going to just completely clone Smithay
and modify smallvil and run my debug prints there.

2026-05-05 7:07AM

I wasn't sure if Smallvil would even work, but it does.

It was a little strange seeing it actually work,
and the fact that it was actually a smooth experience
(not just in performance, I also mean a lack of glitches and stuff).
I ran Smallvil in Smallvil, and it worked, which was surprising for some reason.

Well, this makes it clear that "my" code is to blame,
not my system.

But this is good, it means that my goal is achievable.

2026-05-06 6:29AM

The minimal example actually can run Kitty as well,
I am going to use that.

Even in minimal, there are things that aren't absolutely necessary
to run this, ilke data device (which handles drag and drop).

The minimal doesn't use Event Loop, but I'd eventually like to.

9:12AM

Yooo, it is printing the pixels!

2026-05-06 11:37AM

Oh mah gyahh, it's literally saving to a picture!
This is surreal.

(You know it's bad when I am so surprised my code works)

I'm going to commit this now.

2026-05-06 12:29PM

I guess I should step outside of Smithay land for a while and
figure out how to do images.

2026-05-13 12:02PM

I could just use the `wgpu_canvas` crate,
but it' feel better to do it all from scratch...

Hmm... I think I should just use `wgpu_canvas`.
Other than learning and fueling my ego, I don't see any benefits to writing this code from scratch.

2026-05-30 4:00PM

I think this is the first time I'm working on singularity since I graduated.
I got it running easily on my fast new laptop,
but since I have uncommitted changes here on my old laptop,
I guess I'll work here until it's done.

2026-06-11 2:30PM

I updated stuff and had to fix some wgpu errors,
but now they're gone.

You know what, I'm just going to commit this point.

...

I am going to get the image working ASAP,
I don't like working on this so I'd like to get it over with quick.

I'm going to create an image viewer app.

Okay, I wrote the image viewer, but it isn't yet displaying.
I think I need to update the winit backend.

But btw, I am using Helix now, and it's actually really nice.
I didn't even realize it was written in Rust until someone said it in the forums.
It uses `ropey` to store and modify large strings, which is cool.
As I am using it, I thought of a kinda new idea for a workflow:
We start with a file manager, as the user hovers over different files,
there is a preview of the file on the right side.
(So far, this is how Helix works, kinda.)
But, unlike Helix, you are able to move your mouse to the preview
and start scrolling the preview.
(This is not yet implemented in Helix afaik,
but there is a [PR](https://github.com/helix-editor/helix/pull/15854) for it,
and thinking about extending this PR is where I got my idea from.)
Furthermore, you can click on the preview and edit it,
and you realize the preview box is just the full-on editor app embedded
to the side of the file explorer.
Well, that's neat for having all the tools you'd like for editing,
but you can even detatch this editor from the file explorer,
and just run it as a standalone app.

I have not yet finalized how this would look in terms of the Tree view,
but I think it won't be too hard.

Another application of this I thought of while using Helix was for going through references
of an object while coding.

Well, I'll get my head out of the clouds now.
You can test the image viewer app with `add_child image_viewer examples/smithay.png`.
I'm actually goign to implement Pasting into the command hub as well as a test command,
where `test image` would run `add_child image_viewer examples/smithay.png` and such.

...

I am actually sad about this now, but I might need to just do the image rendering myself
with my own simple shader.

First of all, `wgpu_canvas` isn't documented, and secondly,
I think it might be incompatable with the rest of my existing GPU code.

2026-06-12 2:51PM

I am going to read the
[texture tutorial](https://sotrh.github.io/learn-wgpu/beginner/tutorial5-textures/#shader-time)
from LearnWGPU.

2026-06-15 12:48AM

I am still relying on the old large triangle
so for every single object I render,
I need to do calculations on every single pixel.

There are many things I could optimize.
First, somehow calling the GPU to render multiple objects
at a time, though that might not be possible for different
types of objects at once.
Though, I could try to think of an algorithm to flatten
the hierarchical UI into a flat list,
then every time a consecutive list of the same type appears,
I could draw them together.
Maybe instead of asking apps to return a hierarchical UI,
I literally give it some `Canvas`
which has context info like container size
and has a mutable reference to the list of primitive elements.
(Later, I could do something even fancier and reorder
non-overlapping objects, but I'm getting ahead of myself.)
I mean, I already wrote the GPU code to use instances,
whose use-case is literally for drawing multiple of the
same type of thing.

Also, I wonder if there is a way to do a front-back approach,
where I draw the fore-most elements and
as I render the later elements,
before anything it first checks if the pixel is already
at full opacity.


...

1:31AM

You know what, I literally have a syntax error
because I haven't defined texture bind group layout yet,
but I am just going to commit this right now
so I can log incremental progress even if
there are errors in these intermediate steps.

Taking this snapshot is partially also motivated by the fact
that GPU textures really seem to be a static thing or something that doesn't change too fast,
and so it is seeming more and more likely
that I will have to resort to some other method.
Maybe I'll look into how Wayland compositors do this stuff.

Also, with all the things I want to improve about UX,
singularity_ui is either very low prio
or just a purely instrumental goal
(meaningless on its own but helps me get to other goals)
at best.
I am currently thinking that it is a valueless hinderance
to the goals I actually have.

I don't think I will implement Singularity/sonamu
as just a window manager though.
I think it serves better as an app,
especially since an incomplete app will allow users
to use other apps when the app doesn't work,
but when a WM is buggy or incomplete,
then it is a much larger hassle to either fix or
start another wm.

Ok, whatever.
I desperately need to catch up on my (lack of) sleep now.
Good night.

2026-06-15 3:01PM

So it seems that this is suboptimal (obviously)
but I'll just try to implement it ASAP so as not to waste
more time.

Also, I think I should ask someone who actually knows GPU stuff
to see what I'm doing unidiomatically.
(Maybe I'll just ask an LLM.)

Also, I might just start committing even failed attempts with WIP
and use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

...

I get the error:


```
thread '<unnamed>' (786096) panicked at .../wgpu-29.0.3/src/backend/wgpu_core.rs:2653:18:
wgpu error: Validation Error

Caused by:
  In a CommandEncoder, label = 'Render Encoder'
    In a draw command, kind: Draw
      The BindGroupLayout with 'glyphon atlas bind group layout' label of current set BindGroup with 'glyphon atlas bind group' label at index 0 is not compatible with the corresponding BindGroupLayout with 'image_texture_bind_group_layout' label of RenderPipeline with 'Image Render Pipeline' label
        Entries with binding 0 differ in visibility: expected ShaderStages(FRAGMENT), got ShaderStages(VERTEX | FRAGMENT)
        Entries with binding 1 differ in visibility: expected ShaderStages(FRAGMENT), got ShaderStages(VERTEX | FRAGMENT)
        Entries with binding 1 differ in type: expected Sampler(Filtering), got Texture { sample_type: Float { filterable: true }, view_dimension: D2, multisampled: false }
        Assigned entry with binding 2 not found in expected bind group layout
```

...

Well, I got it running without any errors but it is just black.
I committed already, so look at the code before this commit.

Anyways, changing the image shader wgsl to return blue on the outside of the image
and red on the inside
has the effect of being normal when there's only text and rectangles (which makes sense)
but the moment an image is on screen, the whole screen is blue.
When I switch out to a screen with only text and rectangles,
the screen returns to normal.
Just for fun, returning the image always returns this blue thing
(different shade from when I manually return the blue)
so idk what that means.
It doesn't really feel like it is a color from my image.

But I think this means that the error is in my math, not in my code.
(Well the incorrect math is written in code, but you get what I mean.)

...

After reading my wgsl code again for both the image shader and rectangle shader,
I am utterly buffled by whether or not the clip position is in pixel space or just from -1 to 1.
The positions are just from -1 to 1 in the vertex input,
and the clip position in vertex output is directly taken from position,
so the clip position should be normalized from -1 to 1.
But when looking at the rectangle shader,
everything else in vertex output, like the origin, size, corner_radius, border_dist,
should be in pixels,
and the direct math in the fragment shader just uses the clip position with all of those.

If anything, it seems miraculous to me right now that the rectangle shader even works.

...

Well, according to ChatGPT, the rasterizer turns the `@position`
from clip space to pixel space.
So when the vertex shader returns clip_position, it's in clip space (-1 to 1)
but when the fragment shader takes it, it's in pixel space.

So this makes a whole lot more sense now.
Well, I *could* figure out the math to properly calculate the tex coords (coordinate of the image) in the vertex shader
and have it linearly interporlate for the other points,
which would save computations as well as memory.
But for now I'm going to continue using my SDF based approach.

4:55PM

holy shot, my code works

I wanted to talk about how majestic the 200x200 fuzzy tree looked,
but I didn't in the previous commit to stay consistent with the GenZ all lowercase attitude
I was trying to invoke.

I'm going to just clean up my code, seperate the winit backend file
to sub files.

5:22PM

(Btw, I just want to say that I'm at the library rn,
and I've been super locked in and productive.)

Hmm, I pushed my 5 commits, and I got this:

```
remote: GitHub found 1 vulnerability on mathkimchi/singularity's default branch (1 low). To find out more, visit:
remote:      https://github.com/mathkimchi/singularity/security/dependabot/1
```

This seems fun, let's see what it's about.

Ok, it's just saying to update `rand`.

I might be blind but none of my Cargo.toml's use rand.

Running `grep -r "rand" **/Cargo.toml` returns nothing.

Looking at my errors more, it appears to be in Cargo.lock,
so I think it is from some other dependency.

Running `cargo tree -i rand@0.7.3` (i to invert so I can see who needs rand as opposed to who rand needs)
shows everything that is using rand 0.7.3.
(Should be at least 0.8.6.)

Well, printpdf was the one that needed it and I am already on its latest version,
but luckily I am not using it rn, so I will just remove it.

I'm going to check all the modules that I can upgrade.
I'll do `cargo install cargo-update` to get it,
`cargo install-update --list` to list outdated stuff (even though it seems to only list
a small subset of outdated stuff),
and finally `cargo install-update --all` to automatically update everything outdated.

Bro never mind, this is just for cargo binaries, not for dependencies.
That's why it listed so few things.

Allegedly `cargo install cargo-edit` should do this when I run
`cargo upgrade --dry-run --incompatible --verbose`.

Okay, well both of these are taking a thousand years (I think cargo install cargo-edit
is getting held up by install-update because they are both modifying trunk),
so I guess I'll just talk while I wait.

Generally, features are split into needed or wanted.
I am going to go further, with features that make Singularity/Sonamu:
usable, good, or special.

Features that make Sonamu usable are the "needed" features.
The bare minimum features I need before I can force myself to use sonamu for daily drive.
Quite bluntly, I don't want to implement these, but these are non-negotiables.
The good thing is that these features should be finite,
and if the MVP is done properly, I shouldn't need to think about these afterwards.
(But of course, I can add new features that will improve upon these necessary features.)

Features that make Sonamu special are why I started working on this.
These are the ideas that are the heart of Sonamu, like its thesis.
These features should be what new users come for,
and these features are what I want to work on.

Features that make Sonamu good are going to be the features that keep users coming back.
These should almost feel like the reward for the users.

Without the special features, Sonamu is not Sonamu,
but without the necessary features, Sonamu isn't anything.
(To complete the pattern,
without the good features, Sonamu isn't... good.
Which doesn't sound as nice, and should've been first if included.)

All this to say, that supporting Wayland apps is necessary to make Sonamu usable.
(Or alternatively, I could port a terminal and browser to Sonamu,
but that's pretty impossible in my opinion.)

I was going to somehow twist this into saying
"I'm not going to support Wayland just yet but that's fine because of _",
but I think I have just talked myself into doing it.
Like drinking medicine, I still don't want to do this
but I now understand the importance of it.

2026-06-22 10:08PM

I've been busy moving into Cornell for summer classes.
School's pretty sick.
Dorm room is small but I got a tight fricking setup,
only problem is that my monitor cracked.

I've also been thinking a lot about doing a hardware project recently.
I'm gonna try to get into an intro CAD class.
Birdbath AR seems so easy to do, but it is often the case that
those who are furthest from achieving a goal think it is easiest.
(Holy fire bro, write that down, write that down!)

I also am working in Helix, which doesn't have any features that
really blow me out of the water, at least not features in a standard sense.
(The noun-verb thing is nice, I like it but I don't really think
it is too different from verb-noun, and I don't really have a preference.)
The thing I love about Helix is that it just works.
Well, it still requires way more set up and stuff compared to standard GUI
editors, but geez, this is like a whole world compared to NVim and Emacs.

In the long run, I think Helix is going to need a plugin system.
(I think it is in the works?)
Objectively, supporting plugins is a good feature.
...but, I actually think that Helix not having plugins is one of those things
where an objectively bad decision (according to whatever policy you know now)
led to a better result than the principled decision that you would choose
if it happened again now in hindsight.
Like buying a winning lottery ticket or going to detention and meeting someone cool.
(I've never been to detention, or won a lottery ticket,
but these examples are easier to explain than specific ones from my life.)
Oh, a better example is when you're bad at something,
which motivates you to get better at it,
and now you're really good at it.

What I'm trying to say is that not having plugins made it so that Helix
had to make a really good core, because their entire product is just the core.
...I swear I just wanted to appreciate Helix,
but I think I stumbled onto something very relevant for Sonamu, perhaps.

Anyways, before I was so kindly distracted by none other than myself,
I wanted to say that I kinda just shoved all the compositor logic
into a thread.
It is going to be super performance inefficient,
but my promise to myself was that I'd get this working before I can move on to fun parts.
I never said anything about it being good
or having reasonable performance.

Okay, I'm gonna commit now, and sleep.
The next steps:
- Add the Wayland Applet to the applet registry
- Debug (lots of it probably) (but actually, all the components work, so it wouldn't be the craziest thing if this works without tweaks)
- Allow input
- Debug (way more, probably)
- Celebrate because now I fulfilled my self-promise of actual work and can now "work" on less concrete goals like designing the optimal World Tree design. This has the benefit of allowing me to just day-dream and call it brainstorming. In other words, I am intentionally making it harder for me to actually write code, so I have an excuse for decreasing my coding to thinking ratio.

2026-06-25 11:28AM

I'm probably gonna need to rename crates, but I'll do that later.

...

Ok, I just decided to put it in SDE, but it just shows a black screen.

...

Well of course it didn't display anything, I never set the image.
It now shows a uniform shade of like beige (? idk im not a color theorist).

...

Oh wow, it actually did work, I just needed to press something after waiting a few seconds
for everything to load to have the UI update.
I also got different apps to load, so kitty, konsole, alacritty work right now.
Complicated apps like Firefox and Musescore don't work.
They just open on the normal wayland.

...

Now for input.

By the way, I am working on this rn at a cafe.
My mom wants me to go to my instructor's office hours,
but I am too awkward so I'm gonna procrastinate and tell her I was too busy coding.

Well, it's 12:02 right now and office hours ends at 12:15.
Maybe I *should* go.

...

Ok, I went and I talked to him for advice on what to do over summer,
and he said it's never to late to contact professors.
He also recommended chilling, but I don't like chilling.

Okay, now time to do input.

2026-06-26 8:51PM

My grandma convinced me to get off my ass and so I'm at the Engineering library now.

I wrote hard-coded code to send a keyboard event via the keyboard seat but it's always the Enter key,
no matter what the actual keypress was.

It's getting triggered, but nothing is getting changed.

I think the problem is on the compositor side, and I probably need to set the focus.

Okay, well, I made it set the focus whenever a new top surface is registered to that surface.
It does do stuff on keypress, but it types the letter t for some reason.

Also, it says Client connected then immediately Client disconnected,
which I just noticed and is a little worrying,
but I don't know if that's always been there.
It doesn't seem to be causing direct errors tho, so I'll opt to ignore that.

...

Well Smithay has a Keysym struct that allows me to go
from char to Keysym to u32 (the raw code) to the Keycode struct.
But, doing this with just the letter `e` says it should be the code 101
according to Keysym,
but key press does not make the letter `e` show up.

Also, the [Linux key code map](https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h)
says `e` is 18 and `t` is 20 (28 was what made `t` appear, but 28 is supposed to be enter)
so Keysim, Keycode, and the key map all do not match with each other.

But wait, ok so apparently sometimes shiz is offset by 8,
which explains why 28 was actually `t` which is 20.
So I just have to add 8 from the Linux key code map.
(I don't understand why though,
because allegedly it is supposed to be X11 that requires the +8 offset
and allegedly Wayland and evdev are supposed to have no offset.)

But that still doesn't explain why Keysym is so doo doo.

There's also [this](https://smithay.github.io/smithay/smithay/input/keyboard/keysyms/constant.KEY_e.html)
which says `e` is 101.
Oh, I just realized that this is the ascii value for `e`.

I think I need to get the Keyboard layout and then use the layout to convert the Keysym to Keycode.

[This doc page](https://smithay.github.io/smithay/smithay/input/keyboard/xkb/struct.Keymap.html#method.key_get_mods_for_level)
literally says
"This API is useful for inverse key transformation; i.e. finding out which modifiers need to be active in order to be able to type the keysym(s) corresponding to the specific key code, layout and level."
so I'm optimistic.
Plus, it verifies you have to add 8 from the evdev codes.

Okay, I think it wants me to iterate through all the codes until I find it,
which I'm not going to do.
I'm just going to try doing it by name.

Not too surprised, it doesn't work.

...

Ok, I looked up Rust char to evdev code as well as Winit keypress to evdev keycode,
and google AI says to hardcode both of them.

Honestly, this is going to be easier than trying to
figure out all this barely documented code.
(Not happy about it though.)

But I'll calm myself down by reminding myself that I will just do the lowercase letters,
numbers, and like enter and space, and arrows,
then I'm going to call it good enough with Wayland.
I'm not going to do mouse, I'm not going to even do modifiers.
I won't figure out how to get firefox to work.
I'll just mark this as done/moving on and work on the world tree system
as well as maybe project management.

...

Okay, I spent like 30 minutes learning how to parse Regex and made a build function
just so I could automate going from the
`#define KEY_Q			16` in the C file
to match arms,
for just specifically key chars.
And the build works but I'm not allowed to put a macro as just match arms.

Okay, it works.
I would do three exclamation points, but I'm too burnt out right now.
It's 11:39PM alone in this library, almost midnight, and I'm 20 minutes from my dormroom
in a campus I scarecely know, probably an hour drive from anyone I knew before this week.

Yeah, I'm gonna commit this then walk back.

I kinda don't want to though.
What's stopping me from just sleeping in this library?

That's lowkey hardcore (in the lamest sense of the word),
but I'll save up that manuvre for when I actually need it.

2026-06-27 12:12PM

Right now, the traversal just doesn't work.

For example, make two children and you can't do Alt W and S to go between the siblings.

Huh, I was just looking through `handle_standard_keybinds`
inside of [`standard_keybinds`](./singularity_sttk/src/standard_keybinds.rs),
and I guess I made Ctrl Shift Plus map to adding a child command hub.

And the thing about the command hubs spawned in this fashion is that the keybinds actually work.
I think it's because I spawn them as `RecursiveNodeApplet` of the command hub applet
and when I spawned them before (via command hub prompts),
I probably forgot to make the applets recursive.

...

Okay, so I made them spawn recursively and it mostly works.

A problem now is that when focused on the child of a child,
it doesn't show the grand child, it shows it as the outer child.
(Eg: root -> child -> grand child, when focused on grand child the main view shows child.)

The treeview and input handling work for nested children though
(ex I can keep calling the children and on set_title treeview behaves expectedly).
I think this may be a caching problem.

But speaking of caching, I am taking an OS class and I want to redo
the caching/display updating system that is vaguely inspired by how interrupts work.
I actually had two ideas on how to do the display update system
and the idea I want to do now is the idea I ended up not doing.

For each app and its child, there is a shared boolean representing if the child has
a new update.
For some reason, I don't want to use `updated` or `has_update` or something like that.
I feel like calling this boolean `damaged` or `child_damaged` or `is_damaged` or etc.
Idk why.

The procedure is that when the child app has an update for it's display,
it sets `damaged` to true and calls the `notify_damage` callback.
In the callback, the parent will see if it needs to update it's parent about this
(first, checks if the child is even displayed, and second, checks is the parent already damaged).
(If `damaged` is true, then you don't need to call `notify_damage`.)
It is crucial that rendering logic isn't actually directly called by this,
only queued, since this is how I avoid deadlocks.
(IE, the update display callback should only go up the parent hierarchy.)

Then, when the root decides to process the damage
(again, this should be after the `notify_damage` is finished, not called by it),
it is going to call the `get_display` of the root,
which is going to recursively call the children's `get_display`.
~~In `get_display`, you should get the display (`display = ...`)
then set `damaged = false` then return the display.
This order (of getting the display then setting damage to false)
is because of the case that something changes in between those operations.~~
I had it the other way around.
You need to set `damaged = false` then pull the display.
If you are a parent then you set `damaged = false`
then the child updates again then you pull the display,
then you will return the most recent display and `damaged` will be true,
so you just double render.
But if you are a parent then you pull the display,
then the child updates then you set `damaged = false`,
you will return an outdated display but say it is most recent by saying there
is no further damage.
In general, I guess setting `damaged = false` is the promise that you will
pull the display at that moment or more advanced.
(If you say your laptop has OS 9, you can give the people who bought back then
OS 10, but you shouldn't give them OS 8.
Shaky example but whatev.)
If you're really worried about this, you can just lock the `damaged` variable
for the entire rest of the operation, but that might cause deadlocks
(it shouldn't, but "I have only proved it correct, not tried it" as Knuth said).

If continuous updating is desired, then leaving `damaged` as true is fine.
Because of this, there is actually a final step after pulling the display.
If the child damage is still true, then your damage should be re-set to true.
(I don't think lock is necessary.
Also, I don't actually care too much about continuous updating bc I don't think
they're too important anyhow.)

Well, that was the explanation.
I'm gonna go on small side-tangents now,
just little tid-bits and ideas.

In the old model (with just the updates but no shared boolean;
well, for caching applets the holder kept track of the child),
the plan for continuously updating displays was to call
update at the end of every return,
but I think this is inefficient.

Now that I learned what's going on under the hood,
I want to further change the system,
replacing the entire main loop with a fully event driven
architecture, similar to the Kernel.
But, I kind of don't want to open that entire can of worms right now.
(I learned how to implement these things, but more crucially,
I learned that I would do want to implement them.)
(Actually, that's a joke. I do really want to implement context switches
and interrupt handling and multithreading and scheduling.)

Another aspect of my architecture I might want to change based on OS design
is the recursion.
Processes do have a hierarchy of parents and children,
but actually, they are more or less unstructured in their implementation.
The OS stores the relevant parts of every process in a PCB (process control block),
and a PCB can either be on the run queue (scheduled to be run),
waiting queue (is waiting on a blocking call),
actively running on a core, or on the finished queue (waiting to be deleted).

So, processes, while from the user perspective might seem to have hierarchy,
are all managed more or less equally by the OS directly,
and the hierarchy is symbolic not how it's implemented.
(Technically, the hierarchy *does* matter for spawning, waiting, and cleaning up,
but I mean for the bulk of things.)
This is good for efficiency since it just lets the OS take care of the OS duties
of scheduling and context switching and stuff.
It doesn't require each parent to act as its mini-OS to run its children.

Threads also give good insight, since they kinda have two different implementations
where one is closer to hierarchy (though not recursive) and the other is flat.
The hierarchy-ish approach is user-level threads,
where the OS doesn't actually know the threads that are running.
So, the program kinda has to implement the context switching and scheduling and everything itself,
so the program re-implements for threads what OS already implemented for programs.
This means that the context switching happens entirely in user-space so it's faster.
Also, there's a nice modularity to it.
But, the alternative kernel-level threading, where the OS almost treats each thread
as a process but using TCB instead of PCBs,
requires less redundant code, and is simpler for the OS to schedule.
(In user-level threading, if one thread blocks then the OS thinks the whole process is blocking.)
So, modern OS pretty much just do kernel-level threading
with some hybrid approaches as well apparently (idk what that looks like).

I might need to take a page out of this.
Though hierarchy is elegant conceptually, it unfortunately might not be very efficient
and even worse, might prevent me from implementing certain features.
This really struck me for tree-modifying operations,
like pluck/place and somehow popping an embedded app out.
Idk tho.

...

5:46PM

Okay, that was a whole day's worth of yap.
Time to code.

Since the parent doesn't ever have to change the child's `dirty` status,
I'll just make it so that instead of having a mutex for dirty between parent and child,
the parent just calls the child's `is_window_dirty` method, which is way simpler to implement.
(Just adding a method to a trait as opposed to worrying about who-makes-what and when to pass it.)

Okay, after all that talk, I think I'm not going to do this...
Yeah, I realized this isn't really a feature and now it stopped being exciting.
Also, I think this is just going to introdudce a bunch of boilerplate
without any real gains, especially since a lot of this logic is already in caching app.

...

6:32PM

What a waste, I'm just going to work on fixing the nested bug now.
I'm sad.

I made every recursive node applet print title and focusing state,
and that seems normal.
(Testing with set_title helps.)

I'm printing when recursive node applet returns display,
and it seems like the second layer just isn't being called.

Oh mah lord, it was just a caching problem.
I damage treeview but forgot to damage display on focus change.

That was a simple fix.
I'm going to commit then disable (comment out) the debug logs
and then I have some UX improvements I realized the tree system needed
while using it for debugging:
- if I'm on the focusing state and press something that isn't captured, I should just focus on inner and forward that keypress
- the indent is too large (especially with the current fontsize and on my laptop screen) (I think it is 4 per depth rn but it also varies depending on if its focued or not)
- finally implement a custom display for world tree?

BTW, I don't know why the old code didn't update on subsequent keypresses, but wtv.

I made the standard indent just one character, but its weird bc focused and parents of focused have extra an char.
It might be a little extreme.

By the way, I am considering if it would be better to jsut copy HTML
or even entirely use HTML/some UI library, but idk.
I really need to figure out how sizing is going to work.
Maybe I really will just do a ZUI but a question is
what dimension proportional text is going to be proportional to.
Whatever the case, if I want to do anything more than just blind text-based UI
(that doesn't even do things like wrap),
ie if I want Sonamu to be anything more than a laughingstock,
then I really need to redo the UI system.

Ok, I'll commit the current changes before I start getting lost in the brainstorm sauce.

...

The good thing (or maybe bad) now is that I get to (or have to)
just think.
This is what I so often end up doing,
sometimes it feels like thought paralysis,
and overall, I wish I could just get things done and accept
a suboptimal architecture for the sake of getting shiz done.
But in this case, I am telling myself to think.

Interface Studies is a good channel, but it (and most other conceptual UI videos)
doesn't dive into the implementation.
In some ways, it is straightforward to just get UI done for specific cases,
but I want to make a general framework.

The Wikipedia page for ZUI (which I've read before)
gives examples of apps that have implemented it.
It also mentions WIMP (window, icon, menu, pointing) interface
and the idea of a post-WIMP paradigm.

I think it *is* absolutely helpful to go over existing UI frameworks.
Even if I don't know the actual implementation, just knowing how it is used is plenty of insight.
- Clay (the goat), I haven't used it but this one I actually know how it works bc it has a YT video explaining how the dev (Nick Barker?) made it
  - I am kinda envisioning something more complex for Sonamu, but this is a good starting point and backup
  - Similar paradigm to my current system, where you just have functions that return the UI tree and idk how input works
- Vanilla HTML+CSS
  - More complex than my current system, ex: you can put arbitrary logic inside the UI elements (like you can attatch arbitrary events to UI)
  - I mean, if it wasn't so related to JS, I think I might actually like it. Also, it is verbose but that doesn't really matter if we are generating the UI instead of hardcoding it

2026-06-30 3:37PM

Okay bro, wrap it up.

I'm just going to do dynamic sizing for now,
I'm not rehauling the whole UI system.

I know I should probably make a new branch for this since I'm rewriting an interface,
but I'm so confident this will go smoothly that I'm just not going to do that.
(Marker for future hindsight.)

Oh, I guess I forgot to mention how specifically I'm planning on doing dynamic sizing.
I'm simply going to pass the container size (in pixels, which isn't the most robust generalization
bc maybe the app wants something to be relative to the entire screen size or in physical units,
but they can cry about it)
when I ask to get the UI element.

Right now, since its summer with no friends or a j*b,
I actually want to implement stuff over brainstorming.
Usually, the brainstorming is the fun part and I feel like I never have the time to do it,
but having too much of something makes me want it less I guess.

Btw this is the github issue I created:
[#36](https://github.com/mathkimchi/singularity/issues/36).

2026-07-01 10:28AM

Bro, week 2 of OS, we've just been doing concurrency.
Ts is too easy with Rust bruh,
just wrap everything in `Mutex` or `RwLock` gang.
Or `mpsc`, but I would actually be interested in how that's implemented.

I'm going to just work on a code editor instead of making the tree interface look good
(wow this is kind character growth but also like a disillusionment,
but also not really bc I actually want to implement the editor).

I already have an editor, but I'm just going to make a new one.
I *could* make a new crate like I did for the wl compositor,
and that would be consistent behavior, but I'm just going to put it in `singularity_standard_apps`.

...

Uh, I just got a question wrong (I could've gotten the answer but I didn't think through it thoroughly enough).
Maybe I should pay attention.

2026-97-03 5:29PM

TODO: I should figure out how to use Condvar to redo the whole system
(I don't like how jank it is, I want it to be even less inheritance-y and use dependency injection/composition instead)

Also, I've been looking at Rust GUI frameworks for inspiration,
and I think I like `iced`'s system over `egui`'s immediate mode
(where you just have a state, the state changes, and you redraw the whole UI each time).
Egui is conceptually very elegant and I would probably use it in whatever project where additional features/constraints are not needed.

But, I'm doing the code editor now, so I'll lock in on that.

If I just wanted a text editor that worked, it'd be trivial,
I already have one.
The problem is that there are many ways of implementing even the simplest things
like cursor position (store line and column or index?) that I'm going to do research
on what other editors do.

Resources:
- Helix's [`Document`](https://github.com/helix-editor/helix/blob/master/helix-view/src/document.rs#L141) seems to correspond to the text editor portion of it.
  - They really do store the text as just a rope
- Cursor location seems to be [`Selection`](https://github.com/helix-editor/helix/blob/master/helix-core/src/selection.rs#L417)
  - The main takaway for me (for no selection or multi cursor magic) is just that `primary_index: usize`
  - But in the good lord's name, what is `ranges: SmallVec<[Range; 1]>`? Specifically, wth is `[_; 1]`? They don't even explain it in the docs. Maybe it's just to make it an iterator or something
- [Zed's Rope/Sumtree blog](https://zed.dev/blog/zed-decoded-rope-sumtree) just talks about rope and sumtree's implementation which I don't need because I'm just using ropey

I'll continue citing specific sources I find,
but knowing it really boils down to Rope and usize is encouraging.

...

I added the editor to the applet spawner registry and made it a test shortcut and it ran,
and I probably should've committed there,
but I wanted to fix the cursor not showing up by changing the foreground color.
But then I got distracted and made a GitHub issue (#39) to do a custom glyph renderer
because Glyphon can't handle backgrounds.
While making that issue, I decided to check if I was using Glyphon in CharGrid
jsut to make sure I wasn't saying something wrong in the body of it.
And as I did it, I saw an error with CharGrid doing:
`default_color: glyphon::Color::rgb(fg.0[0], fg.0[1], fg.0[0],),`
even though it should clearly have been indexing 0, 1, then 2,
so I decided to just do a singularity_ui::Color to glyphon Color function,
and in doing so I realized that the other code that was converting
from singularity_ui::Color to glyphon was giving argb to glyphon when it actually took rgba.
So I committed just the UI bugfix.
Then, I ran the example again and now it does show the cursor.

I don't know why I wrote all that down.
Whatever.

There is a new weird bug where going left works as expected,
but going forward sometimes just skips a bunch.

2026-07-05 1:29AM

I am on GitHub mobile as I walk from the library to my dorm.

I am writing because I remembered that my gread grandfather died.

I thought I was supposed to cry. But I don't even feel guilty that the only strain in my eyes are caused by fatigue.
I am just too tired to care about how I'm supposed to react.

But even I, who feels so little right now, find it a necessity to talk to someone about this.
Yet, I have no one I'd like to tell this too.

My family is too different from me and worse,
their understanding of who I am is too different from who I have become.

I can't approach any of my friends with this.
I don't know why, I just can't.
It's just that I don't want to throw a pity party for myself.
This is my life to deal with, and I am worried that if I show I am vulnerable like this in front of someone else,
they will have too much control over me.
I am not worried they will take advantage of their power, but... I don't know.
It's like, whoever I talk to this about, I would want them to also come to me when they're going through stuff,
and I don't really have that relationship with anyone in my life right now.
I had friends kind of like that during school, but I guess I am doomed to grow distant
from everyone I don't have an environmental bonding to.

Anyways, I don't want to write this in my diary because I don't want to be completely talking to myself.
I am putting this entry in a public Dev log because I am not bringing this to anyone
but there's a theoretical possibility someone comes across this.
I am hoping that this is just lost in all the documentation that no one's going to ever find this.
But there is a difference between sealing something and concealing it.

In certain subjects, Singularity or Sonamu or whatever I call it is a closer confidant than anyone in my life,
and for some, everyone in my life combined.
This might seem weirdly parasocial or flatly dystopian to an external observer,
but I have long noticed that I am constantly disappointed by people's lack of reliability in contrast to my code and music.
In conjunction to my apathetic grieving, I must be seeming inhuman.
But let me assure you, hypothetical reader, and really myself, that I am still a social creature.
My projects will always be here for me, but they will never fill my need for social interactions.

In fact, it may be that my current state of social deprivation is the greatest factor in my inability to feel grief.

2026-07-05 7:27PM

Okay, moving on, I'm going to just continue down the list I made in
[#37](https://github.com/mathkimchi/singularity/issues/37).

First, I should fix the cursor moving bug on the right arrow.

...

The issue was that I was more or less doing `cursor += cursor + 1;`.
Oof.

...

Next, up and down arrows.

2026-07-07 11:15AM

I might start a new project soon for independent AI research
or making a social media platform if I end up not being able to find a lab.

Honestly, the reason why I want to do a new project is because
I've been watching documentaries and interviews and it seems maybe it's worth giving
AI coding a shot.
But, I don't want to ruin singularity, so I want to try it on a new project.

Anyways, insertion took me a minute to write.

I'm lowkey distracted texting and just doing beuraucratic setup for college.

Before I display lines or do scrolling (they are pretty related features),
I think I should figure out how to actually display the cursor properly.

This is [issue #39](https://github.com/mathkimchi/singularity/issues/39)
by the way.

I thought about it, and yeah, I'm just going to just use CharGrid.
I previously deprecated it once I started using glyphon and got the ability to
render a whole string at once.
I probably thought rendering a whole string at once would be much better in terms of performance,
but looking at it now, the difference will probably be negligable if not worse.
The benefit with a more hands-on approach to CharGrid is that I know each character will be a fixed size.

Previously, CharGrid was a `Vec<Vec<Cell>>`,
but I am going to make it store the width, height, and just one layer of `Vec<Cell>`
with the invariant assumption that its length is width * height.
(Probably won't directly assert this at runtime, but if this invariant is violated,
I can't gurantee what will happen next.)

...

Okay, I changed CharGrid and the biggest effect was that TextBox had to be temporarily removed,
and becaue of that I also had to remove Text Editor (this one is likely for good though).

I'll remake the textbox widget after I get the CharGrid to work,
but I don't think I can use it in CodeEditor so there'll be a lot of repeated code.

I'll commit now though.
I'm also goign to close issue 39 not because it's really finished-I didn't really change anything-but because
it vague and really just a reminder for myself to think of ways of improving this rather than a specific fix.

...

Now for using CharGrid in CodeEditor.

...

Okay, so it works, but it *is* really slow so I guess my old fears were right.

I don't know why that's happening, but first I'll fix the fact it shows magenta when I print light yellow.
... it was argb not rgba.
(I literally had that then changed it and I swear on @glolichen's hygine it was in the documentation that glyphon uses rgba but wtv
I guess @glolichen can go a few more years without wiping.)

...grrr the problem could be in the Ropey logic (ropey buffer -> Char Grid) or the rendering logic.

I'm going to bet that the problem is the rendering logic.
Hopefully, the problem lies in the fact I am re-making some config thing each time
and I can speed everything up by just reusing it.

Ok, so I have to use a unique `text_render` each time,
but I can reuse `text_buffer`.
There's not much speedup though.

...

Grr... fuck, believe me when I say I didn't want to do this.
Well, I did think it would be cool, but this is going to be a fucking nightmare...

I am writing a monospace renderer that takes advantage of DMA.

Fuck, why do I do this to myself?

...

Ok, well, I wanted to use something like DMA so zero-copy rendering.

...

I got lost in a rabbit hole of Googling and asking Google AI about what techniques are commonly used.
Eventually, I was talking about the general architecture.

I talked about some general ideas in #34, but I spent over an hour on ts.
I think in the future, I'll have one GeneralPrimitive which is like a poor man's enum,
so I can pass all the instances at once and only need one draw call.

Anyways, I'm going to implement the glyph renderer myself.
And I'll do it starting tomorrow.

I swear I'm not procrastinating because I don't want to implement this.
In fact, I am currently at the stage of over confidently assuming this will be simple.
The reason I can't implement this *now* is because I have to make a presentation tomorrow and it is 8:43 right now.
I think I was supposed to have shared it as well, but can't share what I don't have.

2026-07-08 11:44PM

Yes, fine, I asked Claude for advice.
Specifically, asked:

> I have to render a CharGrid which has width height and a flattened vec of chars with very high performance.
Each character is monospace, so it is extremely simple to know what character some pixel is pointing to.
For this reason, I am considering putting all the rendering logic in the vertex shader and use an SDF.
The rest of my codebase is currently in WGPU, but I hear Vulkan supports direct memory access and am willing to switch to Vulkan if it will 2x  performance.
How do you recommend I deal with the data transfer? Performance is the bottom line.

First thing it said is that Vulkan won't do 2x even if it unlocks DMA,
because the bottleneck probably isn't transfering text.

It actually says the first thing I should do is profile before I even try to fix the issue.
...no.
I'm allergic to profiling for some reason.
I'd love to have it if someone else set it up for me,
and I'm sure I'd make good use out of it,
but I just... idk bro, it's not for me.

I actually was thinking to myself if this was possible, and Claude says
I can't use custom texels (texel is like pixel type),
but I can jank the system with `Rgba32Uint` which gives me 4x32 bit,
which I think should be enough.
(First 32 bits for glyph index, then fg color, then bg color, then any styling in binary.)

Ok, I'm going to close Claude and try to do the rest myself.

I'm first just going to get to the point before actually dealing with drawing glyphs.
Specifically, I'll have it fill the character's entire cell with the fg color usually,
but if the character is a space then make it the bg color.

Resource on storage textures: https://webgpufundamentals.org/webgpu/lessons/webgpu-storage-textures.html

...

Ok, it's 1:06AM and I wrote the wgsl code.
I'll commit then sleep.

By the way, I am making the CPU specify the pixel bounds of the char grid
and so if the ratio is off, then the text will look stretched in one axis.
This is actually something I was already planning on having,
and not just for simplicity to implement on the GPU side.
This will help me fulfill the vision of a ZUI.
(Which I don't know will end up good or bad, but at least I'll be able to try it.)

Tomorrow, I'm going to try to organize the 3 renderers I have somehow.
But the "how" is something I'll figure out tomorrow.

2026-07-10 01:38AM

As you might be able to infer from the time,
I should really go to sleep now.
(Btw, I have a shortcut to auto-type the date and time for me.)

I did some commits without logging in devlog.
This is my goodnight commit.

For this commit, I added the char grid render pipeline,
and when I run, it checks if the types match.
Crucially, I had to change the bind group.
There was no documentation on `StorageTextures` specifically in `wgpu` so I thought it'd be another nightmare,
but it was actually quite easy.
The options were just enums, and I could just look through the enum branches until I found the one that made sense.
The one iffy part was the `TextureViewDimension::D2` because the rust doc says it's for just texture_2d
but I'm using `texture_storage_2d` (which is more like a normal 2d array and often used for things other than images I think)
which is different, but I don't get any errors right now, so it's probably the right one.
I'm not going to hope it's the right one, bc I'm saving up my karma rn.

Ok, gtg sleep.
Goodbyeeeeeee

2026-07-10 10:18AM

As I am writing my third shader, I am beginning to understand the WGpu code
(the tutorial probably explains everything well, but I just don't like reading).

For a single draw call:
- Set the pipeline
- Set the buffers in any order
  - Vertex always, instance usually
  - Do textures with bind groups (idk what they are though)
- Do the draw call

Hmm... I was thinking about it, and I might later consider using `texture_storage_3d`
for less-jankedness instead of `texture_storage_2d`,
or I might go entirely the opposite direction and use an array or something 1d,
but idk if those actually improve performance.

...

I wrote all the code to theoretically render Char Grid
and I squashed all the compile-time bugs,
but of course, when I spin it up and open an editor,
it crashes.
I'm not sad about that, it is to be expected.

Also, char cell is a mess rn bc I have an internal vs pretty version.

...

Ok, time to read the run-time bugs and squash 'em.

I'm not going to do a new commit per bug squashed.
Or should I?

The first error was that the bind group descriptor and bind group layout doesn't match.
Simple fix, I was passing in the image bind group layout instead of char grid bind group layout.

Next:
`Usage flags TextureUsages(COPY_DST | TEXTURE_BINDING) of TextureView with '' label do not contain required usage flags TextureUsages(STORAGE_BINDING)`
I think I just replace the `TEXTURE_BINDING` flag with `STORAGE_BINDING` flag.
(It pretty much tells me what to do lol.)

...(do the fix and run again)

Wait what?
It doesn't error?
That was way sooner than I thought!
I mean, it just shows a blank white screen, but still, this is progress.

...

By the way, it displayed all yellow. I had my blue light glasses so I couldn't tell.
If anything, the really tragic part of this story is that it is still super slow.
I mean, once it loads, it seems to be going pretty fast,
but on the first run, you notice a lag that is at least a second long.

Also, my laptop fan starts running, which wasn't happening before, I don't think.

Ok, I'm going to eat lunch now though, it's 12:43.

Since the spaces aren't showing anything, I'll check if they are really the value 32.

I made it output a random color generated from the char_type,
and it's uniform, so I think maybe that's the problem.

Let me also just try making it always return bg.
...yeah, with this, at least the cursor should have something different, but no...

...I made it print the pixel's position relative to the glyph,
so for each cell, the top left should be black and the bottom right should have full red and green values.
Right now, red seems completely gone, and green shows one large gradient downwards.
I manually made the char cell size 10, so we should see 100 cells showing a repeating gradient,
but clearly it's not working.
I will commit here.

2026-07-10 06:02PM

...I might be a goofball, I was doing:
`let char_idx = vec2<u32>(tex_coords / vec2<f32>(in.grid_size));`
instead of
`let char_idx = vec2<u32>(tex_coords * vec2<f32>(in.grid_size));`.
Ok, well, now it is doing almost expected behavior,
but horizontally, it's only 2 boxes wide when it should be 10.
I'm gonna commit now though and go play soccer with random people
(that's a good thing bc I'll meet new ppl).
I'm also 7% battery so it's good I'm ending now.

2026-07-11 12:13PM

Hmm, the right side actually is showing a tiny portion of a third column.
I might be telling it to draw on a wider area than it should be.

...yeah, I think that's it, if I hardcode a width of 100 px,
everything seems to be expected behavior.

2026-07-11 01:15PM

You know what, I'm going to make a testing script.
This won't be a fully automated unit test bc it's just going to output an image
and you have to visually confirm it.

First, I'm just going to get rid of the demos rn because it is error-ing.
They are honestly liabilities.

Making a tester is actually going to force me to organize my code better.

2026-07-11 04:14PM

As I was looking at a part of my code,
I noticed:

```rust
let Some(state) = &mut self.winit_data else {
    return;
};
```

and I guess this is syntax I knew then forgot,
because if I wrote that now (before being reminded),
I would've written:

```rust
let state = if let Some(state) = &mut self.winit_data {
    state
} else {
    return;
};
```

so in my comments I wrote that I should maybe submit a clippy to auto change
`let v = if let Some(v) = ... { v } else { return };`
to
`let Some(v) = ... else { return };`

But to my disappointment this is already a thing,
and I just had to enable `clippy::pedantic`.
So, I'm going to enable clippy::pedantic and try to fix all the lints.

...it's not showing up on Helix's diagnostics picker,
but a bunch of stuff is showing up on `cargo clippy`.

Oh, I can change my helix config with a check command.
By the way, I have always wanted a faster way to cd to directories I use a lot,
and yes, Sonamu will fix this, but I also realized I can simply make a shortcuts directory with symlinks.

...

The specific lint wasn't showing up, and I needed to enable the lint in every workspace.

As I was updating the Cargo.toml's, I noticed that I'm on rust edition 2024.
I should make it 2026 eventually, if that changes anything.

Fuhh, I typed `lint.workspace = true` instead of `lints.workspace = true`
for all the crate Cargo.toml's.
I'm going to do a search and replace:
`find */Cargo.toml -type f -exec sed -i 's/lint\.workspace/lints\.workspace/g' {} +`

Fuh, I put it under `[packages]` (I hate toml).
Let's see if this is the fix:
`find */Cargo.toml -type f -exec sed -i 's/lints\.workspace/\n[lints]\nworkspace/g' {} +`

... oh dear lord, 2208 warnings and suggestions.

I am going to go over all of them.
Yes, you heard me right.
But, I'm not going to go over all of them individually, for the sake of sanity.
For each suggestion that I think is mid, I'm going to disable the entire warning type.

I'm not going to run clippy's batch fixing thing,
because that might have unintended consequences.

`cargo_common_metadata` actually was behind most of these,
allowing it put me from 2206 to 628 errors.
It is for things that are nice to have/recommended for crates,
like License and stuff.
I'll do them later.

628 seems small compared to before, but it's still a crazy number.
Never mind, I made a mistake when allowing that.
Because clippy ignores order, there was a contradiction and I had to manually set the priorities
of the rules.
Now I'm at 1175, so `cargo_common_metadata` was still responsible for over a 1000.

2026-07-11 05:55PM

Ok I'm at 738 right now,
but I realized that for some things like `use_self` (that uses `Self` whenever applicable),
I trust clippy to autofix.

So, I'm going to commit now (I think clippy fix wants a clean repo,
which could be overrided, but it's a good idea) and autofix use_self.
I'll try the command `cargo clippy --fix -- -A clippy::all -A clippy::pedantic -A clippy::nursery -A clippy::cargo -W clippy::use_self`.

...

Okay, that took 7 seconds (starship auto times everything),
and I didn't see any errors.
It *did* notify me of extra warnings like dead code, unused, and deprecated.
I mean, I *do* think adding an underscored before all the unused is a simple fix,
but I only want to do `use_self` right now.
I've been avoiding GitHub Desktop, but I'll use it just to check what it changed.

...

Okay, (bruh why am I so formulaic with my Okay's) so even though it warned me of extra stuff,
it only changed `use_self`, and it did it how I expected it to do it.
Cargo still builds normally, tests seem normal (I never rly did anything with them in the first place),
and running runs normally.
I'm at 544 warnings now, not bad.
That means 194 things resolved, and I think each thing had one warning and one hint,
so that should be 97 changes.
That sounds like a lot, but it makes sense.
Looking at `git diff --stat`, it seems that there are 91 deletions
(additions also includes changing the DEVLOG),
and there were probably some lines with multiple changes like `let middle: Self = Self::avg(Self::MIN, Self::MAX);`
(made up example).

Welp (trying to avoid `Okay`), I'll commit.

Bro, lmao, I wrote:
`#[allow(pedantic)]` for the module where I copied all the macro impls,
and I got another warning because I didn't say `#[allow(clippy::pedantic)]`.
I mean, it makes sense, but I still find that funny.

`semicolon_if_nothing_returned` is making me ponder.
In general, I think it is right,
but sometimes, it is actually more logical to not have a semicolon.
For example, when I'm implementing a trait for a box of dyn trait,
then I actually do want to not have a semicolon to indicate that the inner function called is
the same as the outer function being defined,
saying that if the inner function *did* return something, the outer function would return it as well.
I am just going to leave this warning right now for those cases.

With things like `if_let_else`, I don't see why they would prefer `map_or_else` over
a simple if let else.
I think both ways are probably equally performant and clear,
and they just chose this to set one thing as a standard.
I mean, I had a phase of using closures and maps whenever possible, so I understand.

Hmm, I think it's because it lets me not have to define a variable,
which is easier to code.
I think there's an argument it might be harder to read,
but that is negated with as single comment.

There's also all the must_use suggestions.
I mean, I'll do them as I go and I might just automatically apply all of them at this point.

The `const` when possible is definitely appreciated.
You know what, I'm going to make a shell script for all these little things.

Ok, (damn, I did it again) I'm at 447 errors, I am going to commit then run the auto fixer.

...

106 insertions (not counting README) and now I'm at 237 diagnostics.

I disabled nursery warnings because it's just for experimental stuff.
It puts me at 208 diagnostics, so it didn't really do much.

Some of these lints like `match_same_arms` are just opinionated stuff,
I'm going to be more skeptical of these warnings starting now.

Ok, I'm kinda done now.
There's 16 diagnostics.

2026-07-12 12:11AM

Ok, so where was I before I started this?
(I kinda wasted today's code time on these warnings, but whatevs)

One final tweak that has been on my mind this whole time:

```rs
// for ui_event in state.input_queue.try_iter().collect::<Vec<_>>() {
//     state.process_ui_event(ui_event);
// }
// more idiomatic implementation than the above, which also allocates new memory in the collect
while let Ok(ui_event) = state.input_queue.try_recv() {
    state.process_ui_event(ui_event);
}
```

I hope the comment and reading the code can explain why this is good.
The original impl at one point did show a warning, but I guess it was in nursery or something,
because it doesn't show up anymore.

I feel disgusting committing because I feel like I committed too many times today,
but you know what, that sense of shame is how society controls us, and I will not allow shame
to oppress individuality and just let it happen.

2026-07-12 01:50AM

... hmm, so before fixing that, I was trying to remember where I was
before doing all the clippy stuff, which I did bc I got distracted from doing
refactoring, which I took the time to do before doing tests
(specifically drawing the chargrid output to a jpg file)
because I was rewriting the Char Grid renderer myself in GPU
because Glyphon was to slow and didn't meet my needs for the text editor I was working on.

In other words, I need to do the testing right now.
(That sentence just reminded me, I have my OS midterms Monday and I should be sleeping
and I haven't even began studying.
I really want to get an A+ though for the important reason of fueling my ego.)

Well never fricking mind, I'm giving up on testing.
I never wanted to write it anyways, and it seems like drawing to an image isn't as simple as just
telling the GPU the target is an image.
At that point, I would need to modify my GPU code to have a version for Windows and a version for Images,
at which point, drawing to an image might not accurately reflect what would happen if I rendered to a Window.

I'm just going to debug the Char Grid normally.

... Oh brother, I said the px width was num chars wide * font size * 2
bc I was looking at some code above where I said
we could fit at most (px width / font size) * 2 amount of chars.
Bro, I needed to do px width is num chars wide * font size / 2.

...now it's showing 8 full boxes wide and sometimes a sliver of a ninth column.
The rows are working as expected, they have been working for a while.

With fontsize 1, rows are still working, cols are still usually 8.
I did realize if I stretch the screen fast enough, it shows all 10 columns,
but the get reset on the next event.

Wait, I think I know why this whole thing isn't working.
I don't think I ever remap the display container size,
so each applet thinks it can draw on a rectangle the area of the entire window.
The x axis wasn't experiencing a logic problem the y axis didn't have,
it was just that the toolbar is on the left, so the main applet gets a smaller width than expected,
but there's nothing other than a tiny border above and below so the height was pretty accurate.

Bam, I'm Sherlock Holmes, I can go to sleep now
(actually, I really want to shower after walking around Ithaca).

2026-07-12 01:29PM

I know the error now, but I am not sure how I should go about fixing this.
Right now, the only time the container size of a child should change is when the root node applet
is calling a child.
I could manually calculate the bounds, but that is not going to scale well.

Okay, I think I found a way.
For reference, currently, an app embedding widgets might return something like this:

```rs
UIElement::Container(vec![
    widget_1.get_window(container_size)
        .bordered(Color::GREEN)
        .contain(DisplayArea::LEFT_HALF),
    widget_2.get_window(container_size)
        .bordered(Color::BLUE)
        .contain(DisplayArea::RIGHT_HALF),
    UIElement::text(format!("UI that isn't a widget. Container size is {container_size:?}"))
        .bordered(...)
        .contain(...)
])
```

and you can see that each widget thinks it has the area of the entire container to draw,
but it only gets one half of it (and you also subtract the border size).
The reason why is becaue I am asking each widget for its UI,
then applying decorations which impose size constraints after the fact.
So, the widget can't react to those constraints.

Here is my new proposed idea inspired vaguely by iced:

```rs
layout_helper::contain_multiple(vec![
    (layout_helper::bordered(widget_1.get_window, Color::GREEN), DisplayArea::LEFT_HALF),
    (layout_helper::bordered(widget_1.get_window, Color::BLUE), DisplayArea::RIGHT_HALF),
    (layout_helper::bordered(
            |container_size| format!("UI that isn't a widget. Container size is {container_size:?}"),
            Color::BLUE
        ),
        DisplayArea::RIGHT_HALF
    ),
]).get_ui(container_size)
```

the previous idea I had (and I actually tried this a year back or so),
was to entirely store widgets with their modifiers,
so if the above code only had widget 1, it would've been stored as:
`Contained<Bordered<Widget1>>`
which is absolutely bollocks as the Bri'ish would say.
It led to very messy types and I think the philosophical reason it sucked
is because it forced you to think about the UI Layout every time you accessed widget 1,
even if you were doing something completely unrelated to layout.

I haven't fully solidified the types yet,
but I would have some kind of `LayoutBuilder` trait
or maybe a concrete struct that just holds a `Box<FnOnce(DisplayContainerSize) -> UIElement>`.
(Idk if the name makes the most sense, but I'm a coder not an English major so idc.)

I would like to make a distinction between
having the LayoutBuilders live locally for each Applet's layout then return the calculated UI primitives at the end
(what I'm doing now)
versus each app returning a LayoutBuilder and only calling it at the very end.
I am avoiding the latter because first, lifetime and concurrency issues so it would be hard to implement,
and second, idk I think its really just the first reason.

Another way I've been considering is passing a `Context` to every applet and widget,
and then having them draw to the context,
so it'd be like:

```rs
let mut widget_1_context = context.contained(DisplayArea::LEFT_HALF).bordered(Color::GREEN);
widget_1.draw_to(widget_1_context);

let mut other_example_context = context.contained(...).bordered(...);
other_example_context.add_text(format!("Container size: {:?}", other_example_context.size()));
```

and this is actually kinda neat, I won't lie,
and it would work (I think), and it is similar to what `egui` uses.
The problem is, I think caching logic could be annoying and complicated with this.
(Hence, it makes sense why `egui` doesn't cache either.)
This would be really nice for storing everything in a flat Vector of primitives and still easily enforcing that
applets abide by container size.

Note that in all these examples (except for my first one that doesn't work),
you call the elements outside-in, so you start with creating a contained size,
then you create a border inside that region, then you finally draw the widget inside the borders.

Okay, I'm going to get started on actually implementing LayoutBuilder now after this commit.

...

I made implemented it and I use it in the root node applet now.
Now the horizontal axis is working as I expect but the vertical axis is too short.
It looks like the char grid has the same aspect ratio as the overall window,
but since there are no blockers above or below, the extra vertical space isn't being utilized.
I will commit now though.

...Largest fittable size is being called with the window proportions and not the actual
applet size.
That means `get_window` is being called with the wrong size.

... Ok, I found the culprit, in the find_subsize calculation,
I copied the width logic to height and changed variables as necessary
but forgot to change one of the widths to height.

Nice! It works now.

...

Now I'll revert the debug simplifications one-by-one and try to see how far expected behavior is maintainted.
First, I'll use the actual characters wide and tall (grid size) instead of just doing 10 of them.
Now, the boxes should be constant size and there should be more or less to fit the available space.

Ok, that's a commit bc it works splendidly (I mean, it can either work or not work and it works, so that's good).
By the way, I'm also noticing that after the initial lag,
when I resize, it updates very very fast and smooth.

...

Next, I'll change the shader code to do the fg on most char and bg on space.
This was the first added simplification and last that I can easily comment out
(the next thing I need to do is to go from just solid fg and bg to doing actual characters,
which is probably going to be the actual hard part.
Fuhh, everythiing from the past few days was the relatively easy stuff, I'm cooked).

...

Holy crocodidledoo!
It is doing expected behavior!

And Jesus wept, for there were no more worlds to conquer!!!

It's kinda crazy, but I am feeling the same magic as something working first try.
I think it's because I split this into successively grainier subtasks/simplifications
and once the very specific things were resolved, all the seemingly bigger and harder things came together very smoothly.

Now, it's 3:56.
Do I study for my OS midterm for which I have not started preparing for,
or do I arrogantly wake the beast of glyph rendering?
If I choose to challenge the beast,
surely it would be a foolish blunder driven by the excitement of having overcome a much smaller foe with still great struggle.

They call me Alexander, ambition be my folly.

Frick bro, I'm doing it.
I regret studying too much in highschool, so I'm going to to something even more anti-social and code a fricking
glyph renderer for a text editor for an organization app I'm making.

2026-07-13 11:36AM

Ok, well the test was pretty breezy (finished in 40 min) so I was thinking about my own SDF implementation.
But, I think I will just use msdf (or mtsdf).
The gpu logic is as simple as taking the median of the RGB channels for a pseudo-signed distance.
Then just use that as if it was the normal signed distance.

I don't care to implement the msdf generator, so I will use an existing crate.
[fdsm](https://crates.io/crates/fdsm) is the most downloaded msdf crate,
so even though it has weird documentation, I will use it.

I am going to store a glyph for all the ascii characters.
While I think it is stupid that every program would have to do this in theory,
I am just going to generate the SDFs at the very beginning of each run.
In the future, I might implement some shared static resource so every program could just use that
(note: applets don't need to worry about this, bc they aren't rendering directly).
Maybe if I end up making my own OS, I'll do this.

I'm going to subtract 33 from the character id,
and if it was from 0-31 I'll just draw a bright red
and if it was 32 (space), I'll just draw the bg.
So, I'll be storing a 95 (128-33) x32x32 texture 2d array.

Ok, I guess I'll start with just generating all this in the CPU.

Actually, I decided I'm going to reorganize my code first.

2026-07-13 01:20PM

Now to generate the atlas-es.

...

Ok, `fdsm` crate has more than 10 times the downloads compared to the next msdf crate,
which is `msdfgen`, which is just safe C bindings.
But, `msdfgen` seems just better in every way like documentation and usability
except for the fact that it isn't pure rust.
I don't really care about that so I'm going to switch to msdfgen.

...

Now I have the bytes as a vec u8,
I need to write the code to send it to the GPU.

I am going to try to put as much of the code in the initialization,
so I don't have to copy over data each render if I don't need to.

It is 3:32, I wrote the initializer code for the atlas bind group,
but it errors at the SDF generation.

Ugh, msdf uses f32 per channel by default, but I feel like u8 is more than enough.
I think `klyff_msdf` crate can do u8 automatically and also runs on wgpu,
but for now I'll just render with f32.

...

Ok, so I fixed some minor things like the type problem and using storage binding
(despite what Google wants me to believe).
It runs now without errors...
until I actually have to render a char grid.

I forgot to update the f32 in the bindgroup and texture.
I also forgot to use the new byte size when calculating bytes per row.

Ok, so now I'm getting that the Char Grid bind group is incompatible
with the atlas bidn group.
This is where I should be, so now I can continue,
and the next thing to do is actually pass in the atlas.

Wow! Nice, it runs now.
Next is to use the atlas in the GPU.
Here's to hoping that it's as simple as I think.

3:55PM: The end is right here, I can feel it.

Ok, I wrote the wgsl code and it's saying the sampler type doesn't match.
~~I'm just going to fix it before committing.~~
Nah, I'm going to farm those commits.

... I might be a goob.
Google was right when it said texture_2d_array expected texture binding flag,
I changed the wrong thing.
When creating the bind grtoup layout, I said it was a storage type.
My bad gang, it's chill.

Anyways, I'm getting this error:
`Texture binding 0 expects sample type Float { filterable: true }, but was given a view with format Rgba32Float (sample type Float { filterable: false })`
and I was trying to figure out what was wrong with my code,
and it turns out that Wgpu can't filter (ie interpolate) f32.
Like, bruh.

Well, I'm going to be using `klyff_msdf` now.

...hmm, 19 downloads is pretty scary.
I want to give it a chance, but I just can't risk it.
For now, I'm just going to manually remap this to u8.

By the way, I think even if the texture was originally u8,
the GPU can automatically turn it into an f32 image.
I don't know if I want that though.
But assuming that happens, I think 0.5 will be the boundary.
Oh, the Unorm type does that, I see.
I think it is ultimately necessary because it must interpolate.

Oh my lawd!
It actually renders!
It looks straight out of a horror movie but I'm going to commit now.

I think the simple source of the problem is that the threshold is wrong.
The sign is also inverted for some reason.

Another reason why it looks like a ransom letter is because I think it is stretching
out glyphs to use the most space as possible.
I am not going to do that since this is monospace.

Hmm, bounds still sucks, but at least it isn't the worst part now.
There's still anti-aliasing, the background, and making the user of CharGrid not print the newline.

I'm just going to use smooth-step for antialiasing,
so things `e` away from the boundary are a mix of both fg and bg.

So it does seem to be antialiasing, but I think the problem is still with the bounds.
The characters are just so spaced out right now, especially horizontally.
And they are also horizontally squished for each individual.

2026-07-15 04:16PM

Yeah, I had to take a break.
It is just so demoralizing that the text is so ugly after all this work.
I am going to commit the previous change I made,
which was using the global bounding box for the auto shaper.

I am using ascent descent and advance instead of global bounds to Google AI's suggestion.
Printing this, the ratio seems to be 1:1.933.
I don't think that really changes anything, but I'll try using a 12:23 font aspect ratio.

Ok, I think I need more anti-aliasing.

I think the problem is that the blurring of anti-aliasing is currently twice vertically than horizontally.

Some example msdf sources on anti-aliasing:
- https://github.com/Chlumsky/msdfgen/blob/master/README.md
  - From the original guy that invented msdf
- https://www.fractolog.com/2025/01/msdf-fragment-shader-antialiasing/

all these examples are in glsl, but the conversion is very straightforward.

Ok, so it's actually impossible to get the pixel distance from boundary
if I just have a stretched distance.
I would need a vector, but you know what, it's fine.
I just implemented (mostly hardcoded) my own math bc the guides were using derivates and stuff.

```wgsl
// The gpu maps all distances to [0, 1] since these operations are meant for rgb
// I think the 0 and 1 bounds mean that it is PX_RANGE far from boundary
// I think PX_RANGE was in terms of the texture grid (like the 32x64 or 64x64 grid the dists were stored inside), not the actual pixels
let signed_dist = median(msdf_values.xyz) - 0.5;
// now in terms of normalized texture units, where unit length 1 is the dist of 1 char
// (it's impossible to know the actual distance bc we got stretched dist, so I'll assume it was diagonal)
let screen_uv_offset = PX_RANGE * signed_dist / (vec2<f32>(32.0, 64.0) * 0.70710678118);
let screen_px_offset = screen_uv_offset * vec2<f32>(textureDimensions(sdf_atlas));
let screen_px_dist = length(screen_px_offset);
let opacity = clamp(screen_px_dist + 0.5, 0.0, 1.0);
return mix(bg, fg, opacity);
```

it doesn't work because I lose the sign.
It's fine, I can just do some algebraic manipulation.

Ok, I think I can just multiply signed_dist to screen_px_dist after doing the length.

Never mind, Fter thinking about it, I don't think ts is going to work.

... Wait, wtf?
It still looks disgusting, but less so.

I tried multiplying the distance by 10 just to add some last-ditch fixing efforts, and it actually decreased aliasing
(which makes sense in hindsight).
But without aliasing, it actually looks readable.
You can tell it looks like a gremlin or something (bc its so jaggedy),
but I guess this proves the potential of this technique at least.

With a multiplier of 0.1, it thinks everything is close to the bound,
so all text has an opaque yellow bg, but the text actually looks good imo.
Kinda blurry, but it actually looks like there's a chance someone wouldn't question it if they saw it.

I'm actually going to try to push through and calculate the actual scale to multiply by.

...

Holy forking shirtballs, the text actully looks normal!!!

This is the Aha! moment where I know that the final product will work, but it isn't the end just yet, so I won't celebrate just yet.
After fixing the End of Line and the bg colors,
I'll make it so that a char grid fills up its entire boundary and test it with a single char displayer.

2026-07-15 08:31PM

Ok, I made the tester.
It errors when you press enter, but I don't really care.

So, I think I'm done with rendering glyphs.
Now I can reflect.

I decided to render text myself 8 days ago on Jul 7.
It is now Jul 15, 8:34 PM.
After 53 commits (including this one; `git rev-list f867bbc..HEAD --count` returns 51 and I am including that commit + this one)
and in total:

```sh
> git diff --shortstat f867bbc
58 files changed, 3642 insertions(+), 2016 deletions(-)
```

including 771 DEVLOG line inserts (counting to before this sentence)
with a now 16720 LoC codebase,
I can say I'm kinda proud and feel simultaneously like I overcame a huge challenge
and also that I just wasted a large chunk of my summer and overall sanity.

Welp, time to commit and not think too hard about if this was worth doing.

> We choose to go to the Moon in this decade and do the other things, not because they are easy, but because they are hard.
>
> \- JFK

2026-07-15 08:48PM

I kind of want to jump straight back into editor development,
but I'm actually going to take back and reflect on Sonamu's features.
As I was doing the glyph stuff, I realized that people don't understand how much effort I put into Sonamu
when I just say I'm working on an app that organizes other apps.
So I'm going to add a list of technical features in the README.

2026-07-16 03:19PM

So I'm looking at the git issue [#37](https://github.com/mathkimchi/singularity/issues/37)
for this one, and the next feature is scrolling,
which I was weirdly excited to do (tbh, I was looking forward to anything except for GPU rendering).

I just need a line re-mapper to remap screen line to text line.
I guess it's simple now, but the cool thing will be when I add folding later.

2026-07-16 11:38PM

Scrolling done.

Next on the list is undo/redo history, but I'm going to skip that for now.
(It seems harder than I'd want, though I hear that Ropey actually makes undo and redo history easy to efficiently implement
bc a Rope is immutable.)

So, I have graduated from the "Basic text editor" features to the "Nice features".
I'll start with modes.

I have been theorizing the best ways to do this,
with additional integration with a custom keyboard layout, but I'll keep it simple for now.
Since I like Helix, I'm just going to use most of it's keybinds.

I'm going to add the first Char Grid flag.

2026-07-17 12:17PM

I added a style flag for the cursor and also modes, but the modes don't do anything.
I thought the style doesn't work, but I'm gonna commit now,
because I think it just looks like it doesn't work because right now,
I am inverting the color for the cursor to make a block (which should be Normal mode)
and simultaneously adding the cursor line (insert mode effect),
and it just results in a slightly thinner block.

I'll add mode switching and then change the cursor appearance based on the mode.

The ui_element.rs is kind of a mess,
so I'm going to make the CharGrid stuff its own file.

The cursor doesn't show up at the end (when it is after the last character),
because I draw the cursor when I draw the character with the same index as the cursor.
This is very simple logic that is nice because it works even if I change how characters are displayed.
I *could* just append an invisible space to the end on render or have an if statement for the exception,
but Ropey already provides a line thing that seems perfect for this.
I'm going to just calculate the line of the cursor
and then draw the cursor when I draw the line with the cursor (as opposed to when I draw the char).

2026-07-19 10:16PM

I just got Niri, and within a minute of using it,
I realized I really liked it.
Here is what I wrote in my log for my configs:

```md
Also, I'm going to try using Niri.

I'm not just hopping onto the shiniest new thing (which happens to be written in Rust),
KDE has just been really slow and unresponsive for me.
Pressing the meta key makes the command bar pop up over a second late,
and everything with it seems to be processed with some noticable amount of lag.

There's a [NixOS wiki page on Niri](https://wiki.nixos.org/wiki/Niri/en)
so I'll just install Niri and use their recommended home manager setup.

...

Woah.

Just based off the basic commands and the three finger swipes,
I already know I'm going to love Niri.
Honestly, I want to use a lot of it as inspiration for Sonamu.

I think waybar uses awesome fonts, so I just have to install that.

To be honest, I'll admit that I was initially threatened by how good Niri
is because it effectively implements a lot of the "feels"
of what I was going for with Sonamu.

But, Sonamu still has features that are nowhere to be seen
in any DE or compositor, so I think I should take this as a good sign
that the modern desktop experience still has much to be gained
from a redesign.
And until Sonamu is done, I can tell Niri really is good
from just 10 minutes of playing around in it.
This also is giving me the confidence that if I actually give good
Wayland compositor support for Sonamu,
it might actually be used by other people just like Niri.

Ok, well I have to do my OS hw now.
```

If you're wondering if I did my OS homework,
no.
Well, yeah, kind of.

I'm doing it rn.
(The testing program is running rn, but there's like a state explosion.)

The trouble is, I am studying in the dorm lounge but there is ts couple
that has been talking for like 20 minutes,
so I am looking for something to be locked-in for such that I can
drown out their voice.

"Why not just move?"
you ask.

Well you see, for some reason it's like a matter of principle for me now.
This is really petty, just like when my friend said he had more followers than me
so I spent my free time friending people until I got to 1k followers.

I might be going crazy, but I swear it feels like they're rubbing it in
that they are a couple and I am just by myself coding.
I swear, it feels like that, but I don't want to say the specific things
that lead me to this conclusion because that is even more embarrasing.

I feel like a jaded old man in a christmas movie right now.
A skilled writer could probably write a short story about this.
How the jaded guy is trying to pay attention to his work,
which he has led himself to believe is the only way to happiness
by adopting a transactional world view.
But through his mind, we get details about the guy
and what he's hearing and then we realize that ttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttt

...uh, my laptop deadass just died so I ended up coming back to my room.

BROOOO, I swear if this isn't evidence of their mockery I don't know what is:
They were like "I should go", "before you go, \<starts a new conversation\>"
like 10 times,
and as soon as I went back to my room, they were like:
"Ok goodnight."

Bro, this is the same type of evidence Alex Bale pulls out as the ultimate proof.

(As Chuck McGill)
This is some kind of sick joke to them,
the mockery of the eternal singles.
They do it out of a twisted sense of superiority.

But maybe me thinking that is a revealation of my own twisted sense of inferiority.

Idk, but before my laptop died, I was going to say that
we (the readers) slowly get to know the couple better through the jaded guy,
like what their lives are like and their inside jokes and their love languages,
and then we realize that he is telling us these things because
he has also become extremely invested in their story.
And the writer would mix this in with some backstory of the character.

The old guy only views relationships transactionally
so he could mask his disappointment when people drifted away from him.
And the conclusion would be that he longs for a relationship way more than he'd like to admit.

Wow, there's like so many levels to this because I am me,
but as the writer of this story, I am trying to analyze myself from a higher, rational perspective,
and the character of the jaded old man is a reductive representation of me.
Essentially, the narrator, subject character, and reader
(when I say "we" the viewer) are all different representations
of the real me.

Dude, maybe I really should write a story about this.

As for the practicality (how this premise would actually be set up),
I feel like a stage play would be interesting
(I took a playwriting class and I really liked it).

We start with a guy sitting alone at a round table in a cafe
and the light is just on his table.
He picks up a call and says,
~~"yes, I have the story idea for the book you wanted".~~

~~So pretty much, it's a writer, sitting, working hard, in a library or park,
or I don't know, a cafe (lights turn on and show he's in a cafe
with obvious sign that says "_ Cafe") could work too.~~

~~The dilemma is that~~

New try:

*pick up*

Have I been working?

No, because this couple keeps making out
five feet in front of me.

*lights show couple, he continues to stare at them, they don't react*

I don't know, do you want a story about a guy
trying to drown out the voices of this obnxiously flirtatious couple?
Well, for some reason, that's the only story in my head right now.

Fine, so we have a writer, sitting, working hard, in a library or park,
or I don't know, a cafe (lights turn on and show he's in a cafe
with obvious sign that says "_ Cafe") could work too.
Point is, he's in a public place where people go to work,
and it would be very inappropriate for a couple to make out.

...Ok, end scene, I don't want to get lost in the deep end.
I'm not going to make this a seperate commit.

2026-07-20 09:16PM

I wanted to discuss my new idea for a new framework based on thinking about CSD vs SSD,
but I want this commit to have actual code changes,
so I'll do showing line numbers which I hope is easy.

(By the way, I asked @glolichen for their thoughts on CSD and SSD,
and like a true Arch head and vimmer,
they responded "Window decorations are bloat".)

But the new architecture idea is that basic features (analagous to decorations,
in Sonamu's case it will be mainly the tree management)
are implemented by the server by default,
as opposed to providing it to clients as a toolkit and hoping they use it.
(Later, this will also allow me to deal with permission and stuff.)

Another difference is that the client-server relationship will be closer to two seperate entities communicating than caller-callee.
Previously, the server pretty much called the client, even though when I made it, I was imagining the Client and serverhandler being seperated.
Looking back, this was kinda inevitable with me calling the serverhandler the BasicApplet or NodularApplet.
I think I meant this to be a proxy but 
The things I still liked about this and would like to maintain is that the caller had the option to see the applet's type
(useful for widgets, though maybe I should just make widgets a seperate type, yk what I'm not going to go out of my way to let widgets be applets and I guess we'll see what happens)
and the fact that it supported both active and reactive applets.

So, how am I thinking of actually do this?
I will make communication go through a concrete struct `SonamuMediator` (naming is rough)
that stores callbacks for both sides (for safety, I can later make a client side and server side object that holds a shared reference to the main mediator
but only allows operations that the client/server should be allowed to do).
The server creates this, passing in default callback implementations.
If the client does absolutely nothing, then the applet will just look like a blank screen.
The client can act by calling the default client functions without needing to replace server functions,
and this can either be in response to a server function (for a reactive applet) or from another event source/just on a loop (for an active applet).
If the client wants to be a reactive applet, it can replace the default functions that the server will call (server functions)
with reactive functions.
In this way, a reactive applet doesn't actually have to "exist";
the server gives the mediator to the client initializer and the client initializer just tells the mediator how to simulate an applet
instead of creating a seperate applet entity that uses the mediator.

If you think about it, other than deadlocks and implementation details, a reactive and active applet is actually pretty indististinguishable from the server.
The difference is that reactive applets react to the server, whereas the active applet might be reacting to some external event source.

For example, take both sides the display protocol.
The mediator holds a boxed `DisplayGetter` (a trait?) which the server calls to get the client's display content.
For the default protocol (CachedDisplayGetter), the Mediator stores a copy of the display content,
when the server asks for the content, it just returns that.
(TODO: use Rc or Arc like Ropey to avoid needing to clone?)
The CachedDisplayGetter provides another function as well (not a part of DisplayGetter trait)
for the client to set the display content.
(The request to parent to update is ommitted from this example,
but I think that can be ironed out in a straightforward manner:
just have an extra is_updated bool as well as storing the server's damage callback.)
A standard active app that commits every so often can use this.
A lazy app that only wants to draw when called can implement it's own `DisplayGetter` and call the Mediator to update it.
(NOTE: all these operations should be in lock,
and another note is that from the Mediator, the client won't be able to call the `set_display`
as it is an additional thing implemented CachedDisplayGetter not in the DisplayGetter trait,
so.)
(Sidenote: my spelling has been really bad recently.
Like typing clien't or traight for trait.
Maybe I am just more distracted or simply typing faster.
I thought the latter was true, but after checking monkeytype, my average wpm was previously in the 90s and now it's about 80.
Actually, I think it's just that I was playing shorter modes and swapped to a longer mode.)

This is actually similar to one of the ideas I was proposing for a UI revamp
but didn't end up doing because it was more work than it was worth.

Anyways, I have to make a similar interface for every component of communication
(display, tree hierarchy (use one module for view and traversal), event handling, later: commands/actions, session management, closing).
Previously, the goal was to somehow support arbitrary event extensions and stuff, but I imagine a great Sonamu that doesn't need that,
so I won't think about it.
(You could say that the current refactoring is also unnecessary, but I believe it will speed up development.
For example, the `AppletSpawner` implementations have a lot of boilerplate in my opinion.
Also, standardizing the tree hierarchy and by default not doing it recursively is probably good.
The Wayland applet portal sohuld also be standardized and not recursive, so there should just be one Wayland server.)

Ok, I really gotta take a leak now (biological type, not memory).
I said I wanted to do actual code for this commit, but this is enough brainstorming for a commit.
I guess this also means I'm going to put the editor applet on hold and refactor a crap ton of code.
I'll commit then make a new GitHub issue for "refactor SAP communication with mediator pattern".

2026-07-21 11:50AM

I am looking at the existing code and I don't know why I put the interface between the server and client
in singularity SAR.
It should really be inside singularity SAP (singularity applet protocol, currently inside common),
that's the whole point of it.
Wtv... I guess I'll try to properly organize everything while refactoring.

2026-07-22 07:17PM

Ok, I've had a hard time focusing lately for some reason
(probably bc I am in my dorm and not library, also I've been playing a lot of guitar)
but I got this initial prototype along with a Reactive example.

I'll commit now.
Actually, I would like to add that it worked first try
(well, the first time I tried to run, so I'm not counting compile-time errors).

...

Now, time for an Active applet that does the same thing (just display time)
but I'll have it running on a seperate thread.

...

Active applet was fairly straightforward.
The one annoying thing is that the client initializer takes a box of itself
bc of some `Sized` thing.
(If you want to call a `func(self, ...)` of a `Box<dyn Trait>`,
you either have to change the function to take `func(self: Box<Self>, ...)` or add a `where Self: Sized`,
but the latter option kinda prevents you from having a `Box<dyn Trait>` as far as I've tried.)

Now, I'll do the input bc it will tell me more about if this protocol will work
than implementing tree which is pretty similar to display content.

New perspective (just a different way of thinking about each type):
each component like `CacheDisplay` right now is just one communicator for a protocol,
`DisplayGetter` is an interface for one direction (implemented by client (or left as default) and used by server) of the protocol.
`SonamuMediator` holds a bundle of these communicators.

Ok, I'm going to actually fix the indenting on the readme and commit this.

...

2026-07-23 02:15PM

Now time to implement event handling.

I was thinking about it,
and there is no reason that the server has to give the client a default implementation.

Instead of the current system of the server making the mediator with default impls,
cloning it, giving a copy of the mediator and the concrete default implementations to the client initializer,
and having the client initializer either:
re-implement protocol (using the mediator but discarding the default impl)
or just using the default impl (which doesn't use the mediator),
there's a much less convoluted way.

We often hope that the simplest solutions are easiest to come up with,
but sometimes an elegant solution is only earned after much thought.

2026-07-24 12:39AM

...bruh, I had to send some documents before I could write the actual idea.
Why did I write my reflection on not even the idea but just ideas in general
before actually writing the idea?
(Istg, I have a crazy low attention span, but my mom insists I don't have ADHD.
She's so adamant that I don't have it, I suspect that she might be lying.)

Ok, I have to get my laundry out of my dryer.

Just kidding, the above line is a joke on how I got distracted
reprimanding myself about how easily I get distracted.
The paragraph before that line was really me getting distracted.

Well now I'm distracted explaining my joke about getting distracted,
and now it just became meta.
...you can't seriously tell me I have a healthy attention span.

ANYWAYS, the "much less convoluted way" is to just have the client initializer
return the mediator.

Bam!
That's all it took, just one sentence.
But I decided to dance around it for like 20 lines (in markdown).

This solves many of the sources of "uglieness" I was worried about with the way things were looking.

Ok, I'm not going to actually implement that in this commit,
since I already started making the event handling protocol.

...

2026-07-24 01:01AM

Bro, idk why but today, in my coding hw,
I just gave up on writing good names and used like "wait_for_fight_to_be_done"
for a condvar name.
I ended up changing it bc I didn't want to get points taken off or even risk that.
But in the Sonamu comments, I'm just really not holding back
and not in the edgy sense,
but holding back like the same way you hold in diahorhea.
(I'm not even going to bother looking up how to spell that correctly.)

You know what, I hust decitded that I'm note going to bother fixing bmy typos anymore.

Oh my gah, this pains me,bu t I am hoping that this will make me more productinge because I don't have to woyry about maknig the devlog look nice.

I'll keep capitalizatoin and punctuation though, bc they change the tone.
I feel like spelling (as long isas it can be understood,) just changes the ormality but not necessarily tone
unless you're doing satier (i know how to spell satier, but i'm just leaving that typo).

ok, i guess i'm ignoreing capointaliazation as well.
i'm kinada just meshing all the keybuttons at onexe now,
so i'm actively adding more typos to my typing.
i don't think its really making me faster.

This is like when Kevin Malone did his "smalltalk" from the Office.
And like him, I'll type normally again...
but
hwen me presiesindent, htey see.
theys ee.

...

It is now 2026-07-24 01:27AM.
Running the example (`cargo run --example sap_mediator_sequential`) seems to work.
Even if it didn't, I'm going to go to bed now.
(I'm actually not even in my dorm building rn bc I have to do laundy in a different dorm bc the power's out.)

Kay, toodles.

...

2026-07-24 02:49PM

Now I have to figure out how to do active updates from client to server.
I guess the server can pass something like
"damage callback" to the innitializer.
This is kinda annoying imo but I guess it works so wtv.

Adding that is simple, but now the demo is getting complicated,
so I'll do an actual GUI demo.

Looking at the UI code, I think it is time to update that as well.

I guess I'll commit the damage callback and then start redoing the sonamu UI stuff.

I do want to implement the `tty-udev` backend
(ex: [Anvil](https://github.com/Smithay/smithay/blob/master/anvil/src/udev.rs))
which means I'd be able to run it as an actual compositor
as opposed to it just running as a Winit app.
But, that looks like it'll take at least 1k lines of code,
so I won't do that now.

Hmmm... actually, I don't feel like fixing the UI Display code,
well, I changed the event queue to use mpsc instead of doing an arc mutex mpsc,
but I don't feel like focusing on the display backend stuff beyond that just yet.
Rather, I want to think about how to use CondVar and I am actually quite excited now.

Here's the idea:
the main runner has a main loop, right?
Right now, it's just a while loop that loops through any new events from the UI
then checks if the applet damaged the UI
(and obviously in either cases, it handles those events).

It doesn't immediately look like it, but ultimately, this is equivalent to a spinlock.
Just think about the fact that most of the time, probably nothing new happened,
so we're just wasting a lot of cpu cycles polling for events than actually handling them.
(Analogy: it's like reloading your email/messengers over and over again hoping someone responded.
It would be much more efficient if you just set up notifications and you could sleep or do other chores
until the notif woke you up.)
Luckily, I can use my newfound knowledge of CondVars to do exactly this.
(I am way to excited that I get to use an actual concept from my OS class.)

Now, the other straight-forward method of implementing this would be with mpsc.
You can literally just wait until an event happens, so it seems perfect for this use:
just make an enum for UIEvent and a Damage event (applet updates its UI).
But there is one special condition about Sonamu's usecase:
if there were suddenly two damage events,
we don't need to handle the first one.
With this additional layer of nuance, I have concluded that CondVars will work best.

So just to make this concrete, this is the general idea
(I will slightly implement it differently, but let's start simple):
- there's three entities:
  - the UI display
  - the main loop/main runner (kind of handles communication between the UI display and Applet)
  - the applet (all subapplets all under the main applet)
- Data shared:
  - is_running
  - window content
  - window damaged
  - event queue

So put all the data shared into one Mutex (event queue can just be a normal VecDeque now).
This is because CondVar is inherently couple with a Mutex.
...wait, this won't work because of deadlock.
I mean, I could always just do a trivial Mutex but I feel like that would be an indication that I'm doing something wrong.

2026-07-25 12:00AM

I thought about it further, and I think I will need to update the UI Display logic as well,
and that will help solve the deadlock/mutex issue.

Let me just, lay down on the dorm lounge sofa and try to think.

~~Okay, I'm going to make a system where there's only one thread necessary,
and the UI mainloop is going to be the only loop.~~

2026-07-25 02:26PM

I'm thinking about it, and the crossed out thing above is the opposite of where I'm supposed to be going.

I think even with reactive applets, I should be using threads.

Now that I know what CondVars are and that you can just wait until an event,
I realize that I can just make Reactive Applets have their own threads while still not being wasteful.

I'm actually pretty sure now that I just re-invented how every single app already works on every OS.
Technically, the callback based method could be faster on one core because it doesn't require context switch,
but that is such a minor cost and nowdays, with multi core, it is actually faster to do multithreading.

Ok, so this is what I'm thinking:
the times an applet has to be awoken,
are when the parent sends an event (event_queue is non-empty),
when one of its children updates their displays (one of the `child_damaged` is true),
and whatever custom logic that the applet might implement.

This is the generalized/simplified synchronization problem I am facing:
I have a tree structure, each tree has a corresponding thread.
When a thread is awoken, it needs to lock all its edges.
In other words, a parent and child will block each other.

Preventing a deadlock is actually not necessary in this problem...
As I learned in my OS class, one of the conditions for a deadlock is a circular wait.
As Mr GWK taught me in my highschool graph theory class, a tree is by definition acyclic.

So as long as grabbing a single edge can't deadlock
(I say this because maybe an "edge" is actually multiple objects),
the tree can't deadlock.

So, I guess there wasn't really a problem.
I'm just going to say wait on the parent edge mutex.

I'm imagining something like this for the main loop:

```rs
let mut parent_edge = parent_edge_lock.lock().unwrap();
loop {
    // acquire children edges
    let children_edges = children_edge_locks.map(|lock| lock.lock().unwrap());

    // Do actual processing logic

    // remember to manually drop all the other locks while waiting
    drop(children_edges);
    parent_edge = node_condvar.wait(parent_edge);
}
```

This is different from the standard way of using condvars
because the whole mainloop is in an infinite loop
so we don't need to check seperately against supurious wakeups.

Okay (*sigh*), I guess I'm going to just make every applet a new thread.

You know what, no.
I'm going to just leave it as it is right now threading-wise.
(Little bro is chained to sunk cost fallacy bruh.)
To recap, the applets will have a thread with a main applet loop,
and the UI is going to have its own thread.
Children applets don't need a thread but it is supported.

I will implement the condvar and mutex logic as if each app was a new thread though.

Ok, I'll commit this brainstorm.

I feel like I didn't actually come up with something new,
but before I was trying to use as little threads as possible because I thought it would be more efficient,
but now I know it's less efficient and I'm keeping it like this because I'm lazy.

...

2026-07-25 11:52PM

I'll start by implementing the CondVar logic for the "edge" between the UI and root applet.

When I was storing locks for each component, I used AtomicBool for is_running
and a mpsc for event queue, because individually, these are better generally than mutex bool or vecdeque individually.
But, I decided to put everything into one mutex,
and I am also going to use an `Option<EdgeData>` instead of having an is_running bool.

...

Bro I'm going to abandon all technology and go live in a forest or something.
Winit babysits you by forcing you to use their loop system.
Apparently Smithay provides their own `calloop` crate but doesn't require it.

I think I'll make a custom sync primitive that lets you wait until the type is updated.
For this commit, I'll put it inside the winit impls,
but I think this means I have to change how my crates work.
I was going to just put singularity_ui as a module inside of the common crate,
but I think the play is something more complicated:
I put singularity_ui as a subcrate of singularity_common and pull out singularity_sync
as another subcrate of singularity_common,
then have singularity_common rely on both of its subcrates
and also make singularity_ui rely on singularity_sync.

For now I sleep though.

2026-07-26 01:39AM

Oh my gosh bro, I just implemented this in the UI Display side.
(It doesn't do wait or wake up, but that is lowkey Winit's fault
and that wasn't being done anyways so wtv, there will be no change.)

2026-07-26 07:49PM

What's up?
I'm streaming rn. (It's an unlisted stream so this is more of a rehearsal.)

Using new code on the main loop as well, no compile errors.
Need to fix deadlock or something.

...

I just forgot to notify the condvar on keypress events.
I should really think of a way to automatically do this.

2026-07-26 09:23PM

Ah, I see the problem now, it is because when the applet wants to close,
it tries to change the shared state, but this is called by the main loop while the shared state
is locked by the main loop.

This is because in the edge analogy, the edge between the main loop and UI
should be different from the edge between the main loop and the root applet.

Before I fix this, I'm going to just make Singularity UI a subcrate of the common crate.

...

2026-07-26 10:13PM

I did the crate stuff and abstracted out the other stuff.

Here is the comment explaining the fix:

```rs
/// Because of the "edge" model (look at devlog sometime before 2026-07-26),
/// we have main runner loop share a state with UI and with root applet,
/// but they should be different locks (ie: UI and root applet can not lock each other)
/// so, for redundant information like whether the app runs or not,
/// I will duplicate that information and let the middleman keep them matching
pub enum RootAppletState {
    Running,
    Ended,
}
```

2026-07-26 10:47PM

Ok, it works (closing and everything else like UI event and updating) now!

I think this code will work 99% of the time,
but there are two problems I thought of:

First, I am just cloning one condvar for the two edges of the main loop,
so if this logic is used by all the other nodes as well,
everyone will end up using one condvar.
I did think of a way to fix this by letting one condvar represent one node,
and an edge actually holds two condvars (or maybe 1 and an optional condvar).
Waiting remains simple and the difference is that on notify, it notifies both condvars
(or if you figure out who the caller is, then just notify the other one).

Anyways, that is something that can be fixed in a straightforward way
(even if it might be a little annoying).

The other problem is that just like how I could have missed a UI notification
before I used the `&mut` thing,
in this code the main loop could still miss an update from the root applet.
I am done with Sonamu for today, but I think if I write down all the interactions I want,
I will be able to represent all of them in non-blocking ways.
(Might end up going back to AtomicBool and mpsc.)

I feel like most of this is straightforward except for the fact that I want
rendering to be lazy,
so when the content updates an applet just notifies its parent and then
only has to actually give the content when parent asks for it.
I might have a system where the "renderer" of an applet runs on the parent.
Or maybe I should just force the child to render every time and just set a shared data.

Pros:
+ prevents misbehaving applets blocking the parent and potentially main thread,
+ simple to implement
+ could make the overall system faster if rendering logic takes a long time
+ I already use this system for the main loop to UI communication

Cons:
- suboptimal in terms of allowing unnecessary compute
- sunk cost fallacy
- the current (previous) way is most conceptually elegant imo, because it theoretically allows for the most updated possible content

A very inelegant solution that technically keeps the pros
and negates the con would be to just tell each applet if it's visible or not.
I don't like this though.
(Mostly because of sunk cost fallacy, but also because it means on the first frame of appearing,
the applet will be outdated.)

For the damage tracking, I might also make a sync object that is a cross between
a condvar, atomic bool, and mpsc.
Like mpsc, you could split it to a sender and reciever.
The sender sets it damaged to true,
reciever can wait until damaged is true and can set damaged to false (maybe not instantly though).
(Maybe semaphore could help?)

Maybe I can look into interrupts?
Idk, I'm just throwing out a bunch of ideas.

2026-07-27 12:49PM

Will stream, but I want to write down what I want to accomplish beforehand so I can start fast:

The overall goal is to make Clients run on worker threads (with tokio probably).

I can only do that if I make it so that client can not block server.
This means that I will be removing the `get_display_content` function that server calls for client.
I will use the shared data idea I discussed earlier,
but I have a strategy to minimize unneccessary computation
(where applet draws more than server renders,
eg: an applet that displays time to the nanosecond so it is always updating):

When an applet has new content, it update the shared content
and notifies the server of `content_damaged`.
Later, when the server renders it, it will look at the shared content and render it,
then tell the applet that the content was read (set content_damaged to false).

An applet should update its content when:
it has updates to make and content_damaged is false.

Requiring content_damaged be false will make it so applets don't
generate content more often than server renders them.
I think the content will be outdated by at most the time between frames
plus latency.
I am fine with this much outdatedness, especially since it has an upper bound.

...

It is 2026-07-27 03:08PM.
I lowkey screwed up on stream because I kept just going with suboptimal choices and ugly code
because I wanted to keep the stream going,
and I ended up running into a problem with circular initialization,
which I could've solved with even messier code,
but I decided that was finally too much.

I am going to commit this now,
and next I'll just kinda start a whole new sync primitive for the edge and node system
because I just thought of a nice implementation for it,
and I want to feel productive.

The solution I was talking about is to have
a lock on each node as well as each edge.
When a node waits, it waits on its node lock.
On wake up, the condvar automatically gets its node lock,
but we want to acquire all the edges as well.

Reminder: this gurantees no deadlocks in a tree because trees are acyclic.
(This means two nodes can't have multiple edges either)
Otherwise, a deadlock might be possible if you aren't careful.

~~If you blindly acquire the edge locks afterwards,
you might get a deadlock because two nodes might~~

~~Never mind, from here, you can just acquire the edge locks.
No need to do something fancy with acquiring the other nodes first.
Then you just drop all the edges before waiting again.~~

Wait, no!!!
In the time between dropping the edges and waiting again,
the other edge might change it.
This is the whole problem I was trying to avoid!
Plus, the node mutex wouldn't actually do anything in this case,
which should've been a sign of something gone wrong.

Guranteeing no notifications are missed means
a node can't notify another node unless the other node is actively waiting.
Since nodes wait on their locks, we can know a node is actively waiting if it's lock is available.
This means that if I can grab another node's lock, that means that node is waiting right now.

If we only had this procedure where we grab the other node's lock
to mean holding the edge,
we would now have no missed messages, but we could have a deadlock
if two nodes try to grab each other's locks.
I'm sure there's a faster way of solving this,
but I'm just going to try thinking of a solution that uses the edge locks,
because of sunk cost fallacy.
Blindly saying acquire the edge then the other node would still lead to deadlock.
But, we could say if you can't acquire the edge, then you must temporarily release
your node lock (don't worry about releasing the already acquired edges because there's no cycles).
Don't worry about missing notifications either because we already know that there have been notifications.
...I am just going to use Tokio.

I just realized the language of the setup (nodes and edges) of this is somewhat similar to the dining philosophers problem,
but the problem we are trying to solve (no missed notifs vs no deadlock for dining philosphers)
is very different.

2026-07-28 12:31AM

Bro @Logan-Huang's claude came up with a solution to this with an update counter.
I didn't read the whole thing, but the solution is pretty obvious once you get the concept of the update counter.

A node's mutex lock holds an update counter.
The node thread should hold a num_updates_processed,
and the condition for the condvar can now be whenever the update counter doesn't match the num_updates_processed.
We can now be very liberal with our wakeups.
Specifically, whenever an edge's lock is dropped, we can notify the other node.
(Previously, this would've caused nodes to just infinitely wake each other up when there was no wakeups.)
This deals with the deadlock problem between two neighbors,
because the neighbor who doesn't immediately get the lock can just
drop everything and restart.
(You can't just have it wait on the edge lock while holding the locks it currently has,
because the other node might need to update this node's update counter.)

We also avoid the missed update problem because to update a node,
you need to update its node counter so that node's thread must be sleeping.

Booyah!

I'm going to just commit the devlog and give @Logan-Huang the code file for the solution that I wrote.
I still want to avoid fully AI generated code,
but I still want to let Logan contribute.

...

Wait, I was coding, and I just realized:
why can't I just make this a boolean?

I'm pretty sure I absolutely can.
The logic is pretty similar, it's 12:40AM so I won't explain it.

...

2026-07-28 01:15AM

Wham!
I'm oily as heck bc I didn't shower and I had just cooked beef for lunch
(no carbs or fruits or veggies or seasoning),
but I did it!

I'm going to commit this DEVLOG and send the file to Logang.
I haven't tested it yet, but honestly, I'll blame it on having to send this to logan.

2026-07-28 04:22PM

I am now using the sync node and edge,
but Logang is playing tennis so I'll wait until I can push to or fetch from remote.

Also, I told this problem to my OS teacher,
and they said I should use queues of empty values,
and I realized that's the same as having an update counter,
which can then be boiled down to the current boolean system.

2026-07-28 10:47PM

Logang made the PR, so now I'll just commit and push my code.

My code isn't working.
It's just a black screen that doesn't even quit.

It looks like the main loop constantly fails to get the UI shared data.

But actually printing when UI locks the edge,
this happens before the UI locks anything.

If I call `try_lock` before `wait_for_update`,
it returns some.
So either `wait_for_update` itself somehow changes the state
or whoever is updating changes something.

There's too many things going on at once with three threads.
I'm going to test the sync stuff in a toy usecase.

Ok, I did a simple test with two threads, but it seems to be working mostly.
Wait, on failing to lock an edge, a thread retries over and over again without waiting.

This is because `wait_for_update` automatically lets them through.
I'm going to commit the debug stuff now,
but the solution should be to allow for `wait_for_notif`
that should be called before the continue on lock fail.

Bruh, I couldn't see the debug in the ui because I renamed the crate to start with sonamu
and the env logger was only letting messages from crates that start with singularity through.

I think I've reached the point where I need to redo the Applet interface as well,
because it isn't letting me run an applet's handle ui function in a thread bc it takes &mut self.

Oh my life, I'm going to become a music major and start practicing my begging skills already.
AI deserves to take my job for this.

I just realized I can just use a normal fricking event queue
because I already figured out a system to limit the amount of damages.
My OS teacher was lowkey prophetic with ts.

> Rule 4 of Computer Science:
> Don't try to reinvent the working wheel if it's working.

2026-07-29 12:36AM

Ok, so I'm just going to try to use Smithay's calloop.

Calloop seems like it should work.

I think I footgunned myself by trying to solve a non-existent problem of wasted compute.
The problem Sonamu is solving was never about the computer's efficiency,
so I shouldn't have tried to do something new for it unless I started experiencing performance issues.
The only new thing I really need to accomplish with a custom applet interface
is embedding other apps.
This is different from a weaker widget system.
For example, consider the mini text editor inside a file explorer.
Previously, I wanted a rather straightforward approach of saying the parent applet deals
with compositing the children UI,
but I think I will allow for a more centralized approach:

The Sonamu runner keeps a flat hashmap of all the applets (rn, say applet and surface is 1-to-1).
If an applet wants to embed a child surface, it can just have a UI Element that points to the child surface.
The UI framework shouldn't have to deal with this,
the UI framework only deals in UI primitives and maybe resizing.

This is sounding similar to the Wayland protocol.
Maybe I should just really look into how Smithay does everything.

It seems I can use a `wgpu::TextureView` to represent a general surface,
including a Smithay surface as well (hopefully).

The centralized approach would allow for me to deal with WL apps easier.
Instead of implementing an applet that acts as a Wl to Sonamu layer,
I'm just going to have a WlApplet vs SonamuApplet type.

Okay, so I just went around my UI code thinking of how I'd do this,
and I have an idea for the overall display protocol.

Next, for how the applets are stored on the server (and client-server communication),
as I said, I'll have a hashmap mapping their IDs to the client handle.
A client handle would include the event queue from server to client as well as the shared data for its surface.
A client holds the server handle, which would include the request queue from client to server.
I haven't looked into this, but Calloop's futures::executer might allow for query-requests.
I think for each client, server just adds the client as an event source
(for this part, I can try to look at what a normal Smithay compositor server does).
The server also keeps track of the applet ownership hierarchy
(just simple parent child relationship, differnt from the more robust organizational hierarchy).
Each client should have exactly one parent (or point to root).
I think I'll just let them recursively deal with user input and the organizational hierarchy for now
(though centralized shortcut and action manager would be nice later).
For a sec, I considered if I should just have two layers of trees,
with the outer layer being the ownership hierarchy,
but that wouldn't be able to support things like projects or embedded apps or inner apps. 

I think that at this stage of development, I should just think of Sonamu as a hierarchical WL compositor
that also supports a second type of applets that better hierarchy integration.

Ok, I should sign off now bc it's 2026-07-29 02:17AM
and I had to run to class this morning not to be late.

2026-07-30 07:34PM

I'm feeling kinda overwhelmed now.

I think it would be best to start with the interface,
but I honestly don't even know what a Calloop interface would look like,
so I'll start by thinking what it should look like from the server/runner side.

For each client, the server should be able to:
- send events to client
- react to their requests (hopefully do queue-response as well in the future)
- read client's surface and treeview

In other words, those are what the ClientHandler should provide to the server.

Similarly, server handler should let clients do the converse:
- receive server events
- send requests (later: send queue get response)
- set surface and treeview

I'm just going to do something very basic for the shared memory
and use an EncapsulatedLock.

I guess I'll revive the old sap modules for the server handler,
client handler, and packets.

Ok, to start very simple, I did some basic events and requests.
I'll commit now bc I want to make incremental progress.

I realized that I could also have a seperate event source for each packet type.
There's not really a benefit to this I see for event and requests.
But for queue-response maybe this could be really good.
(I mean, I guess protocol extensions could work with this,
but it's fine bro.)

Also, I think handle makes more sense than handler.

Previously, the client handle would be implemented by the client
and the server handle was implemented by the server.
I think both should be implemented by the protocol itself
so I can have more of the complicated logic hidden from both sides,
and it could also be dangerous to let them directly call each other.

Ok, I implemented the server and client handles.
It was actually very easy.
I suppose the challenge will be in using them.

2026-07-30 08:38PM

Time to redo the runner ig.

I'll start with just one applet.

Looking at Niri, it seems they have like a main [`State`](https://github.com/niri-wm/niri/blob/main/src/niri.rs),
storing the actual logic state (agnostic) + an enum for the backend.
The event loop handle is given on initialization of the state and stored.

To actually run the event loop, Niri just has:

```rs
// https://github.com/niri-wm/niri/blob/main/src/main.rs
event_loop
    .run(None, &mut state, |state| state.refresh_and_flush_clients())
    .unwrap();
```

the third argument, the callback, seems to be called between each event,
but I don't think I'll need this.
Smallvil uses run as well and doesn't have the callback
(it just has a trivial body).

For anvil, (both [`udev`](https://github.com/Smithay/smithay/blob/5fb12b87407b3680135c45d94214c5f1b1d0fbea/anvil/src/udev.rs#L538)
and Winit too),
they implement a main loop manually and use
`event_loop.dispatch(Some(Duration::from_millis(16)), &mut state)`
which means the event loop just handles a single event.

I don't know why Anvil does this, but I'm just going to use `event_loop.run`.

2026-07-30 09:34PM

I started on the server runner.
I think next, I'll need to do the UI.

Bro for a second I thought the UI was already using calloop,
but it is using Winit's own EventLoop system.

I guess I'm going back to the atomic bool for is running,
send-receive channels for event queue (but with calloop not mpsc this time),
and encapsulated lock for display.
I feel like I'm just reverting all of earlier this week.

Good thing I hoarde dead code, but I did remove some stuff.
I'm going to the winit backend winit impls from the commit before where I tried to refactor everything
(maybe I should've used branches, but oh well, can't change the past;
maybe I should use branches now, nah, I'm too lazy):
https://github.com/mathkimchi/singularity/commit/ca9e4c7e7a2feebf34999e5d654f6445c11e9cf9.

2026-07-30 09:59PM

Ok, I set up the UI to hopefully be compatible with the server using Calloop.
Next, I need to actually hook up the server and UI,
so I will make a UI handle for the server to hold
(should just be a bundle of the is_running, event queue, and display).

...

2026-07-30 10:24PM

Done with that, next is the client side.

I actually need to make design choices.
I was lowkey feeling comfortable just turning off my brain.

Okay, so I'm probably going to add some wrapper type to make it easier to use
and harder to footgun, but I'll explain the raw init interaction between client and server.

Server gives the raw client initializer the shared root content holder,
an event receiver, and a request sender.
Client initializer is run on a seperate OS thread.

Wait, that's lowkey it for the raw interaction.

I imagine the client initializer will make its own event loop,
hook up the event receiver to the event loop,
and run it
(so the initializer thread would block until app closes).

Everything else will just be convenience on top of that,
including the `ServerHandle`,
which should really belong in the sttk.

2026-07-30 10:45PM

Ok, just committed the idea, now I'll actually make the raw client initializer interface.

I won't add things like send sync or object safety (for box)
for the initial making of this,
I'll add it as I need to while using it in the server in the next commit.

2026-07-30 10:58PM

I put the client handle in SAR and later I'll put the server handle in sttk.
Neither of these structs are used by the other side, so it shouldn't be under common.

2026-07-30 11:17PM

Ok I made server initialize client.
Next is I guess testing?

I'll make a simple client to test?

I think I'll put an example client in the sttk,
because then, it'd be able to use the tools I make in sttk.
I'm not putting it under standard applets,
bc first of all, it's not an example of how to use a standard applet,
it's an example of how to use the sttk.
But more practically, standard applet has like 20 errors
which means I'd have to fix them before being able to run the test.

Right now, it'll just use the raw protocols tho.

Wait, the errors were in sttk too.
I'm going to make a raw applet demo inside the SAR.

2026-07-30 11:45PM

I made it and squashed the compile errors,
but it immediately panics.

2026-07-31 12:02AM

Ok, it runs now.
I'm going to commit then add very minor improvements.

2026-07-31 12:22AM

Ok, I added quitting.
Speaking of which, I am going to retire for the day.

Tomorrow, I'll try to actually get the old client code working.

2026-07-31 12:25AM

Ok, I took a tiny sneak peek at the err'ing clinet-side code just so I could think about it in my sleep.
(I mean, the few times I dreamt in code or about coding, it felt more like what I imagine acid trips to feel like
and less revalatory, and they lowkey feel suffocating but they're pretty cool and I want to have more of them.
But I say "think about it in my sleep" in a more rhetorical sense or just that it'll be in the back of my mind.)

Anyways, I'm going to set myself the challenge of having the interface be the exact same in the client side.
The two things is having a runner hook and a traight (I'm keeping this typo) for the client.
This might mean some suboptimal code and unnecessary complexity,
but I'll make it work first then make it good.
So I'll split the "Basic" vs Nodular protocols and have the hook be a boxed trait
even though I'll know its struct for no reason other than bckwds compat.
Now I understand first-hand why some protocols are so bad.

Actually signing off, Alvin - 2026-07-36 12:31AM

2026-07-31 07:38AM

Early day (for vacation-mode Alvin)!

I was looking into Wayland display protocol yesterday as I was poopin.
It seems like they have the client request a frame,
the compositor sends a frame when ready,
and the client draws to the frame and sends a commit.

Also the compositor requires clients to ack configure
so changes like resizing.
If client doesn't ack, it isn't rendered and may even be killed.

I just learned I can do `WAYLAND_DEBUG=1 alacritty`
to show all the wayland packets sent.
I guess I'll see if my previous explanation was correct.

I think I have isolated what happens every draw:

```log
[3458889.080] {mesa egl surface queue} wl_callback#48.done(0)
[3458889.109] {mesa egl surface queue} wl_buffer#37.release()
[3458889.275]  -> wl_surface#40.frame(new id wl_callback#48)
[3458889.941] {mesa egl surface queue}  -> wl_surface#40.attach(wl_buffer#37, 0, 0)
[3458889.951] {mesa egl surface queue}  -> wl_surface#40.damage_buffer(0, 57, 55, 92)
[3458889.956] {mesa egl surface queue}  -> wl_surface#40.commit()
[3458889.961] {mesa egl surface queue}  -> wl_display#1.sync(new id wl_callback#51)
[3458890.786] {Display Queue} wl_display#1.delete_id(51)
[3458892.357] {Display Queue} wl_display#1.delete_id(48)
[3458892.391] wl_callback#48.done(128594666)
```

packets starting with `->` means the client is sending it to the server.

Ok I was really confused because I thought the point of wl_surface.frame was to know when it's allowed to draw
so it didn't make sense why it would draw before getting the `done` msessage.

Apparently, the `wl_buffer.frame` is more like "Lmk when you process the commit I'm about to send you".

For my protocol, I'm just going to always send done,
so this frame packet won't be necessary.

2026-07-31 08:41AM

Time to code.

I said I wanted to make this completely backwards compatible,
but I guess I'll change things around just a teeny tiny bit.

First, I'll be reorganizing the modules,
so the import paths should change.
And if I'm changing that anyways, I think I'll rename the runner hook
to server handle and just make it a concrete type.

Ok, well, I actually might need to keep runner hook how it is right now.

2026-07-31 09:07AM

I updated the basic applet and basic runner hook.
Next is updating the initializer, then probably implementing the runner hook
to use the event loop stuff, and then trying to get this whole thing to run.

2026-07-31 10:02AM

Well, I did all o the above by just making RootNodeApplet compatible with event loop.
I don't have compile errors and it technically runs,
though it is running a black screen.

2026-07-31 10:10AM

I just had to fix the UI display logic and it works now.
I know the edge cases in which my current code fails.
(Eg: if applet requests redraw twice and then surface awknowledges,
then it will stay as the old frame.)
And I know I could fix it by storing two seperate variables for
"last_frame_processed" and "new_frame_exists"
but I don't think it rly matters ngl.
Like, yeah, I should fix it but I don't think it is really gonna change anything.

But anyways, I think I'm back where I started in terms of the actual app running and
what the client can do.
I guess the client has more ways to be implemented than before.

2026-07-31 10:21AM

I realized the UI looks jank because I am not doign the resize properly.

2026-07-31 10:27AM

Fixing redo was pretty simple.
I'll do some house cleaning I've been meaning to do,
with squashing some warnings.

First is fixing the lint name for lints.rust.unused qualifications.
Oh damn, that brought me from like 40 to 209 warnings.

Next, I'll run the classic auto clippy fix.
It brought me back to 41 warnings, so I guess it just fixed the unused_qualifications.

2026-07-31 10:34AM

I was confused why it fixed that automatically, since auto_clippy_fix
should be set up to only fix things I manually enable,
but I realized it's because I told it to ignore every clippy lint other than the ones I manually enable,
but unused_qualifications is not a clippy lint.

The auto fix left some weird imports so I just ran autofix again which fixed those (leadingt 31 warnings),
and now I am also going through the remaining ones that are left over
(I think there were two ways to fix them so autofix didn't do anything).
(Or actually, maybe it's because it's the example.)

Now I just have 15 diagnostics.

I'm not gonna narrate any more, this is quite trivial stuff.

2026-08-01 04:08PM

Since I'm kinda able to just work on clients again,
I guess I could make the editor good.

But I think I'd be missing the big picture,
and wasting the whole refactor I spent the last week doing.

Sonamu *needs* almost first-class Wayland support.
It should be able to Wayland apps as well as Wayland compositors,
but I say almost because it will give Sonamu applets even better features.

Issue [#15](https://github.com/mathkimchi/singularity/issues/35) falls under this.
I made a new issue [#44](https://github.com/mathkimchi/singularity/issues/44)
for the general Wayland support.

I lost my passkey (needed for ssh auth), but I guess I'll temporarily use a PAT until I get it back.
Actually, I don't like PATs.
I'm just going to pull a normie move and use a simple password protected ssh key,
but I'll do some crazy password.

Actually, I'm going to do something else but I won't say it here.

I now have a merge conflict from like 24 commits ago.

2026-08-01 10:23PM

I had to run `git pull --rebase https://github.com/mathkimchi/singularity dev -Xours`.
I'm pretty sure I screwed up something with the git repo.

I am going to punch a wall or something;
why did I lose my security key.

I thought my projects were the only thing I could always rely on,
and I literally lost the keys to it.

I should never have come to Jersey.
There is nothing for me here.

I thought I would find things to do with my friends,
but they're all busy or gone.
I am disappointed to learn that,
and I am also disappointed in myself for giving up developing for this.
I also lost my lyrics notebook, so I can't even do that.

I am just going to play the guitar and watch the Matrix.

Actually, I'm going to work on the guitar practicing app I was working on.

2026-08-02 10:21AM

Ok, so I made a branch for this.
I think I need to set up the multi-surface thing
so I should let the UI draw from surfaces.

I think the raw UI should only deal in primitives.

2026-08-05 03:46PM

I've been kinda busy, and I haven't really committed for a while despite making multiple changes.
The last thing I did was modifying the UIElement drawer to draw primitives.

Next is using this code to actually render,
and then converting UI element to primitives.

2026-08-05 04:49PM

I have it rendering, but it's just rendering a tiny square right now.

2026-08-05 05:41PM

I don't know if I explicitly said this before,
but the plan is for the raw runner to display whatever the root applet wants to display,
and the root applet takes care of figuring out where to put the subapplet windows.

Maybe I should just put the root applet inside of SAR.

But anyways, I am going to use a slotmap for the subsurface system.
This will mean it's easier to decouple surface and applet,
but I won't do that yet (I'm lazy).

For each surface, there will be a surface index,
and the runner holds a slotmap of index to surface.
Btw, a slotmap is like an IdMap which I think I got rid of and replaced with IdTree.
The point is, you insert an element into the slotmap and it returns a key like an index,
and you can access or remove by the key.
The nice thing is that insert, access, and remove are all constant time (theoretically).

2026-08-05 06:09PM

I just realized I can just store the subsurfaces.
So it's possible that the list of subsurfaces is just empty.

I also think the UI thread might need to share the device with an Arc if I want to make Textures
that can be passed to the UI thread.

I'm just going to have it so applets can request surface texture with a size,
then the runner thread creates the surface texture,

(Also, I just realized subsurface and texture are different.
Subsurface should hold UIElement and texture should be the wgpu thing.
I've been meaning mostly texture this whole time,
bc the goal is to support Wayland.)

And if I use `TextureView`, I won't have to even store all the textures and map an id to texture or something.
Remember, a `TextureView` is like a reference to a texture.

I don't know if I can turn a Wl surface to Wgpu texture,
so I should probably pause and research that (ie ask an LLM Chatbot).

2026-08-08 12:01PM

Freak, I've been locked out.
I'm just gonna barge in without a plan.
I'll make a new Smithay module inside sar and try to keep all the smithay logic in there.

I'll write down some of the things I learn.

Terminology alert: Buffer vs Surface (in the Wayland protocol)
- Buffer: container of pixel data
  - Directly modified by client
- Surface: rectangular ui area
  - Has an attatched buffer
  - Has a position
  - Can take input
  - Ig this is modified atomically via commit systems

so maybe I should really be storing buffers instead of surfaces,
since I want to do the positioning stuff myself not with Wayland.

2026-08-08 05:32PM

Ugh, it's so annoying that the documentation (and the whole source code on crates.io)
is outdated so when I search something up I might get a result like this:
https://smithay.github.io/smithay/smithay/wayland/compositor/struct.SurfaceAttributes.html
which is currently for smithay 0.7.0 which is super outdated.

I don't wanna just ask LLMs to think for me,
but to do something like getting the buffer from the surface,
there just isn't a good resource.
I can't even look at other existing compositors like Niri because Smithay is just changing so fast.

I don't really have anywhere to put the code to turn a surface into a buffer,
but I'll need it later:

```rs
let wl_buffer = with_states(surface.wl_surface(), |states| {
    let surface_state = states
        .data_map
        .get::<RendererSurfaceStateUserData>()
        .unwrap()
        .lock()
        .unwrap();
    let buffer = surface_state.buffer().unwrap();
    buffer.clone()
});

// call this later
buffer_type(&wl_buffer);
```

With Wayland, the app is started by whatever,
and the Wl server just listens for an app's connection.
This is kinda incompatible with my applets that are started by the server.

I'm going to deal with this (the organization of Wl applets in Sonamu hierarchy)
by letting the focused applet deal with it.

And I was also thinking more about putting the organization logic in the server.

The reason why I can't just have the server manually set 3 levels of
project tree, applet tree for each project, then content tree for each applet
is because one of the content nodes might be a subapplet
or one of the subprojects might make more sense under an applet
(like "send emails" task under "get a job" project)
(generally, I am pointing out cases where a node might be so engrossed in another node's
internals that it would make sense to be inside the other node's inner world,
but because those two nodes are of the same type,
they must be in the same level and the best we can offer is making that node a child node)
and also sunk cost.
Maybe I should also completely decouple the project tree from this;
I should call the outermost thing tasks or sessions
(paralleling a virtual desktop/workspace in other DEs).

2026-08-10 11:04PM

I've been busy meeting friends before heading off to the middle of nowhere.
I'm on the train now.
Babies should not be allowed on trains bro.
They (the parents) should be fined each time they cry or something.

So I need to make it so the root applet can organize (and display)
wl applets.
I already decided to do this with events.
I'll do this by surface instead of by Wl client.

2026-08-10 11:26PM

I implemented the code for letting the root applet know.
By the way, I am still down for having events be sent directly from a central
event router to the applet that is listening for it.
(Ex, most can be sent directly to focused, maybe if focused applet
doesn't listen to traversal shortcuts, it can be sent to the ancestor that does,
and there can be some global.)
But, that is not an MVP feature.

2026-08-10 11:30PM

I think I can just treat the surface spawned event very similarly to
spawn child request inside the nodular applets.
For demo, I don't care how they are placed organizationally,
but for the actual thing, I'll probably combine this with a check for focus and pass down to focused child.

I think I should have a sonamu client for each wl surface.
I mean, that's what the old really slow system was already doing,
so the difference is that the old system had a wl server per each sonamu wayland applet
but now the bulk of processing is done centrally and the sonamu applet is just a wrapper
so wl apps can be in hierarchies and stuff.

2026-08-11 05:40PM

Right now, nodular applet is doing it's own thing,
so I might have to redo it.
Previously, I treated the nodular events as a seperate set of events and requests than the UI ones.
But the standard events system (which I am basing the new protocol for)
is an extension of the UI events.

I think it's time to make applets actually use the event loop and the new system.

2026-08-11 06:25PM

I made the applets actually use standard events and resolved errors,
but now I'm getting a panic instantly.

...

2026-08-11 06:34PM

I just had to change the
`let display: Display<Self> = Display::new().unwrap();`
to
`let display: Display<AppletRunner> = Display::new().unwrap();`
in `SmithayState::new`.
Weird that this isn't a compile-time error.

2026-08-11 06:49PM

Bro, I thought this was a good stopping place,
but the librarian has my card and she's been gone for like 10 minutes.

I'll start planning the demo for the centralized WL apps.

2026-08-12 12:59AM

I don't know why in the code I'm about to commit,
running `insert_source` on calloop's LoopHandle blocks
but the dbg calls inside of it are being printed.

The AppletRunner state shouldn't even exist and event_loop.run hasn't been called.

For context, the code is:

```rs
event_handle
    .insert_source(listening_socket, |client_stream, (), state| {
        let client = state
            .smithay_state
            .display_handle
            .insert_client(client_stream, Arc::new(ClientState::default()))
            .unwrap();

        dbg!("Inserted new client!");

        dbg!(client);
        dbg!(&state.smithay_state);
    })
    .unwrap();

dbg!("Hi");
```

Prints the "Inserted new client!" and the client and smithay state
but then the whole program freezes without printing "Hi".

`state` shouldn't even exist, which is the largest problem.
But also, the callback shouldn't be run until I call run on event_loop
and also I'm not spawning any clients so this shouldn't be called ever right now.
Plus, insert_source shouldn't be a blocking call.

...

Wait, I might be tweaking.
I'm still gonna commit these dbg prints,
but I just realized that if I scrolled up, the "Hi" was printed...
Might be a goon.

2026-08-12 01:08AM

Just committed the full dbg, but the fact that "Inserted new client" is printed at all is still
inconsistent with my expectations of what the code should be doing.
I haven't written any code for spawning clients yet.
Also, I don't know why suddenly the normal window just stopped running either.

Commenting out the insert_source makes the normal window run.
Commenting out just the set env var part makes the normal window show up.

... waaaiittt, this seems obvious in hindsight.
The sonamu wl server was cannibalizing its own window.
Idk why I used that word, but I just mean that when the main Sonamu window was spawning,
it was spawning under the sonamu wl server I created.
That explains both inconsistencies.
I think I assumed winit would not be impacted by the set WAYLAND_DISPLAY.

For the purposes of the demo, I'll just not set the env variable for the Sonamu process,
and when I spawn wl children, I'll just make them spawn with the WL display variable hard-coded to `wayland-2`.

2026-08-12 02:16PM

I updated the `test wl` command to spawn alacritty natively,
and it is currently being registered.
I just have to display the surface now.

For some reason, it isn't registering a new top-level surface though.

2026-08-12 05:13PM

Adding this at least causes an error:

```rs
event_handle
    .insert_source(
        calloop::generic::Generic::new(
            display,
            calloop::Interest::READ,
            calloop::Mode::Level,
        ),
        |_, display, data| {
            // profiling::scope!("dispatch_clients");
            // Safety: we don't drop the display
            let display = unsafe { display.get_mut() };
            display.dispatch_clients(data).unwrap();
            display.flush_clients().unwrap();

            Ok(PostAction::Continue)
        },
    )
    .unwrap();
```

anvil doesn't have the flush_clients, but without the flush_clients,
nothing changes from before I insert the source.

And reading the logs, it looks like the top level surface is actually handled,
and the surface might just be from the fact I'm not dealing with it in the clients,
which is, unexpectedly, expected behavior.

2026-08-13 11:10AM

Google might just be thinking again.
I got two potentially useful results for turning a Smithay surface into wgpu:
- [Lamco wgpu](github.com/lamco-admin/lamco-wgpu) which provides "wgpu integration for Smithay-based Wayland compositors."
- [This Reddit thread](https://www.reddit.com/r/rust/comments/1ojfg29/initialising_a_wgpu_context_from_a_wayland/) which points to [`create_surface_unsafe`](https://docs.rs/wgpu/27.0.1/wgpu/struct.Instance.html#method.create_surface_unsafe) but I don't know how to use it.

Ideally, I'd like to just use `create_surface_unsafe`.

2026-08-13 01:22PM

I think I should make the winit backend use calloop so I can share the wgpu stuff between the ui handle and everythign else.
I tried looking at Anvil's winit loop handle, but I think Niri is much more understandable.

This kinda means I have to put the Winit backend stuff inside SAR,
since the event loop holds a generic which refers to the SAR state.

2026-08-14 09:32PM

I'm doing like a service trip with manual labor tmrw apparently.
I guess I'll try to squeeze in Singularity development,
but I doubt I'll even make a commit until the 18th (when the trip ends).
There's like 40 people sleeping in one large gym,
some typa plague-maxxing.

I was thinking about it, and I don't actually know if it's good to put the UI handle in the same event loop.
The pro is simplicity, and this sence of one large structure,
but the con is that then, the UI handling and the other handlings must run in the same thread.

The alternative I thought of was just putting the threads in their own loops (like before),
and sharing the device stuff as Mutex.

I don't want to think about this too hard, so I'll just finish what I started in this commit
and only consider splitting the threads if/once performance becomes the priority.

2026-08-19 07:43AM

I just realized that the `wgpu` types like Device internally use `Arc`,
so I didn't have to worry about Mutex overhead.
I'm going to commit the "switch UI thread to eventloop" which is unfinished and just share the stuff.

...

2026-08-19 02:36PM

I hate winit, why does it force its event loop system on everyone?

Whatever, I'm just going to use a Mutex of a Some,
but if I do it on the WinitData (currently the Some is of winit data),
then I pretty much won't get the benefit of multithreading.
I considered having the UI thread send the wgpu stuff to the main thread,
but winit sucks so that would require me to add a rx,
but I think I got my current solution:

I use a once lock (which is kinda like a mutex of some) for the overall winit data then,
idk I think that's it actually.

2026-08-22 02:54PM

Might need to do arc mutex option bruh.

2026-08-22 04:08PM

I got it to work by making the device and queue in the main thread.
I forgor why I did all this.

Oh, I need to make the wayland smithay surfaces into wgpu textures.

2026-08-23 10:39PM

I finally got uploading to wgpu to work, but I think there is a CPU copy when I use write_texture.
Now I just need to write the code to write the texture.

I'm going to try to repurpose the image renderer code to work on all textures generally.

...

Bruh, it's like kinda working but nah.

2026-08-24 12:37AM

Hmm, I've been just messing around with formats for a while and I think it looks like the chargrid shader might be being used.

But even when I directly draw the image, it's weird, so I don't think I'm accidently using the chargrid shader.

Claude is suggesting maybe Alacritty thinks it isn't actually being shown so it's not rendering anything.

2026-08-24 06:10PM

Wait bruh, it works when I run kitty.
I think Kitty is more liberal with it's commits.

I'll still have to fix the Alacritty though.

2026-09-04 06:11PM

I think the problem is that Alacritty uses GPU rendering so it is DMA instead of SHM.
But right now, I shoud run into a todo panic when encountering that, so I am a bit confused.

I'm going to commit and start using LLMs more tactically from now on.

2026-09-04 06:36PM

Uhh, it read a bunch of files and didn't actually fix anything.
Maybe I should try to prompt it more specifically.

2026-09-04 06:45PM

What the freak bro.
After discarding the trash changes, I decided to prompt it with stronger guidance:

```txt
No it doesn't fix the gray square.
I don't think the rendering is the problem so I undid the changes.
The BufferType::Dma path should've paniced in the original restored code,
so changing that shouldn't have any impact.
I think Alacritty never actually renders to the shm because
it doesn't realize it is being displayed at the moment.
Perhaps it is waiting for the server to send it something first.
```

This sounds meaner than I realized, but the point is,
it actually fixed it with a surprisingly consise fix
(one new function `send_frames_surface_tree` defined and just call it once,
and the function itself is also like 20 lines.)

2026-09-04 07:15PM

I asked AI how I would best do the event forwarding to the wl app from the
wl surface applet,
and it said to make a new event.
But I realized this is something I already know how to do on my own,
(as opposed to with some of the other smithay problems which stems from lack of documentation)
there's really no reason to ask AI.

I decided on something rly jank,
which is to include a key event queue going from the singularity applet to smithay app
on registering.
It sets the keyboard focus to the place, then does a press then unpress.

2026-09-04 10:04PM

...huh, it is pretty much not working,
but in my testing (consisting of mashing a bunch of buttons),
suddenly, all the stuff I typed in was processed then nothing happened again.

2026-09-22 11:28AM

Just used claude to fix panic on keypress (suddenly appeared on new setup)
and to actually show changes with the wl.
But, it's really slow.

I might refactor the whole codebase with Claude to render like a normal Smithay client.
The custom renderer was fun, but I don't know if it will scale very well.

2026-09-22 04:53PM

I am considering fully managing the hierarchy in the Server by default.
This would also help a lot with the Wayland integration aspect of everything.

Maybe apps could do special stuff like embedding via "interceptions".
In either case, embedding other subapps is a part of my vision but it doesn't need to be a part of the MVP.

2026-09-23 08:30PM

Hmm, Claude is saying that I should keep the current architecture but add an optional
direct hook for when an wayland applet is in focus.
I think this is just interceptions.
But it wouldn't make sense to only do interceptions for Wayland applets like Claude suggests
because they can't handle the tree traversal stuff.

I'm actually going to ignore the general architecture problem for now and
focus on getting rid of the latency from copying display.
(This was another thing Claude suggested.)

2026-09-23 09:32PM

Ok, I started out by adding Numpad Enter keyboard for Enter bc my keyboard on this laptop is weird.
That's just for testing, and I was testing removing the `image::save_buffer` that Claude caught.
It didn't speed things up as much as I wished though.

2026-09-23 09:45PM

Woah, I was swizzling in the CPU, and taking that out as Claude suggested makes this like instant.
The colors are wrong, but I should definitely just have an extra case in the GPU
or just switch the rest of the code to use the same format as Smithay.

I don't even think I need to do the UI rehaul anymore.

You know what, I'm going to just ask Claude to make more of these small changes.

I would also ask it to remove unused code, but I am a bit of a hoarder...
I promise I'll remove it when someone else has to work on this codebase.

2026-09-24 10:15AM

I think I should expand the scope of events.
First, I'll just make it so every keypress is forwarded.
Then, I'll do mouse clicks.

Actually, I'll first make the buffer size for wayland match the screen size.
This should be easy to handcode by just changing a few variables.

By the way, I've been using 800x600 or sometimes something w/ 400 for my placeholder screen sizes.

I'm just going to do a jank fix for now by adding a resize event queue.
I already have a key_event_queue, and I'll need to add another channel for mouse later,
but wtv.

I realized that I should actually be using Claude for these easy changes
more than anything.
I think that's what I should trust it most for.

2026-09-28 10:50PM

I think I'll try to kill a few birds with one stone
by figuring out the best way to do inputs generally.

I remembered that Smithay's smallvil example was run on winit,
which means it should contain an example of converting
winit events into Wayland events.

I guess there are two places I found:
- https://github.com/Smithay/smithay/blob/master/smallvil/src/input.rs#L17 for input events
  - Smithay provides an `InputBackend` trait, which is fair, but I'm gonna just come up with something reasonable to handle everything
- https://github.com/Smithay/smithay/blob/master/smallvil/src/winit.rs
  - For general winit events, including resize, input, redraw, and close

I might just take the lazy route of least change
by just representing input events with
my current representation + the smithay/winit one.
(Smithay would be `InputEvent<WinitInput>` and
winit should be WindowEvent)

Looking through Smithay's code,
the libinput (the most low-level one, and the one actual compositors use)
backedn uses the `input` crate developed by smithay.
I could probably convert winit events to the events in input.

You know what, I am just going to make Claude figure it out.

...

2026-09-28 11:49PM

Claude decided to make its own RawInput thing,
but I didn't like that window resize wasn't a raw input,
and I just realized I could just include the raw key code in the
existing UIEvent::KeyPress and also just have a bool for press or not.
So, I reverted all its changes and manually changed UIEvent.

I'm gonna commit this and then make claude fix all the little associated bugs.

2026-09-29 12:36AM

I manually figured out why recursive node applet was breaking.

The one bug I'm noticing now is that the smithay app doesn't know if a modifier is lifted if it is lifted after switching.

2026-09-29 11:37AM

I thought I had a shared boolean for `was_updated` for the UI between the ui thread and main server thread,
but it looks like I don't,
which is currently leading to both unnecessary updates as well as updates that aren't processed.

I'm going to add a modified `EncapsulatedLock` that tracks updates,
I'll call it `TrackedEncapsulatedLock`.

...

2026-09-29 12:02PM

Ok, it was actually fairly simple, and running `dolphin` shows that it is actually surprisingly smooth.
I handwrote this because I was making a new type with a new interface and that is fun to do.

Using a queue based system would probably be even better, but the goal isn't performance right now.

To be honest, I think this qualifies as reasonable wayland support.

My next task will be figuring out how to run things like firefox,
which open up in the outer compositor for some reason.

2026-10-11 10:47PM

I was asking Claude if there are ways to make my UI Element protocol better,
and it found the [`taffy`](https://crates.io/crates/taffy) crate.
Bevy, Servo, and GPUI all use it, and those are all projects I mad respect.

So `taffy` is just the layout library, which is actually ideal for me.
(I have sunk too much into the renderer, I wouldn't be ready to part with it.)

This will probably make it much harder to do the ZUI I envisioned,
but I really need to remember what is important.

I can replace `DisplayUnits` with Taffy's `Dimension`.
Uhh, there's 91 references to just `DisplayUnits`,
and I'll replace all the other stuff as well.
I'm going to make claude do this, as it is quite trivial.

...

Never mind, it wasn't a one-to-one thing.
They allow storing length (which would be pixels for me) and percent,
but those can't mix.

So it looks like this will be more annoying.

I don't want to just ask Claude to decide how to actually implement this,
because I want to learn how this algorithm works.
I am fine with letting it write code, but I want to be making the choices,
even the technical ones.

Looking at the taffy docs and examples,
it looks like the best way would be to associate each primitive with a `Style`
then build the `TaffyTree`, then get each primitive's `Size`.

(TODO: look into CacheTree.)

Well the time is 2026-10-11 11:33PM and I've been waking up at 6 so I'll go to sleep.

2026-10-12 10:24AM

I slept at 12 and woke up at 10.

Anyways, I don't need seperate cases for contained and container.
I'm going to redo UIElement to resemble a Taffy Node
(conceptually, it is represented in code differently but the idea is the same).
So every node just comes with the style (like size and constraints)
and a vector of children.

I decided to have `UIElement` for like the actual UI nodes
and then a `UITree` for an actual UI with multiple elements.
This is similar to UIPrimitive vs PrimitiveScene.

I also made it so that the Border and Background doesn't contain anything.

This is my prompt by the way:

```rs
Ok, I implemented the UITree and changed UIElement.
Use UITree to represent content instead of UIElement.
Make all the trivial changes you can and when all the changes are non-trivial, wait for my permission.
Or, if you forsee non-trivial changes already, ask me first.
(It is fine if you stop while there are errors, that is to be expected.)
```

I should lowkey turn this into a skill or put it into the claude md file.

...

Apparently Style is not Send or Sync bc it holds a raw pointer or something.
So I'll have Claude make a new Style with only a useful subset of fields.

2026-10-12 07:05PM

Yk what bro, I'm just going to make Claude fix the updating problem.
Bruh, it fixed it in 1 minute and a half.
(My job is gone.)

Yk what x2, I'm just going to make Claude do all the non-major Wayland impls automatically.

2026-10-13 07:46AM

Btw, I wrote the previous commit in Sonamu, and I'm writing this devlog in Sonamu in Helix.
It is pretty surreal.

...

I had to quit because the mod keys weren't working or something.
I couldn't hit escape so I couldn't `:w`, but I luckily rebinded Ctrl+S to save which worked.
Next up is tree management.

2026-10-15 09:57PM

Ok, I asked claude to move everything to ClientHandle, but I used grill-me.
Yk what, I'll just make Claude summarize all the changes:

```txt
At a high level:

- New architecture: every applet now runs on its own thread as a client. The applet tree lives in the root client, and the server holds only that root. Applets change the tree only by sending requests to their node; the old direct hook calls are gone.
- One update pattern: a client publishes its surface or treeview, sends a damage notice, and waits for the reader's ack. One generic type now handles this everywhere instead of hand-written copies.
- Wayland tiles: wl clients are a kind of node directly, so the separate wl applet wrapper is gone. Ctrl+W now asks the wl client to close, and its tile goes away once it does.
- Simplifications: the deadlock and the unreachable resize branch are gone, since the old structure that caused them no longer exists. The command hub, the three active standard tabs and the SDE entry point are ported with minimal changes.
- Testing: everything builds, Sonamu runs with a wl client connected, and the main tree flows worked in a headless test. I haven't tried it interactively.
- Docs: the project docs describe the new architecture, and a refactor notes file records the plan, the decisions I made and the issues found.
- Next: I've proposed a design for handing a client or subtree over to a different owner, as groundwork for pluck and place. It's waiting on your approval; nothing is implemented yet.

ClientHandle refactor

singularity_common::sap
- client_handle.rs (new, moved here from sar): it works from any holder's event loop, and the holder passes a closure that handles the client's requests. Reading a client's surface or treeview also acks it. A closed request channel is ignored instead of panicking.
- shared_state.rs (new):
  - SharedState<T> is a value the client writes and its holder reads.
  - StatePublisher<T> is the client side. It renders only when damaged and keeps at most one unread publish; damage that arrives while waiting for an ack is merged into the next one.
- packets.rs:
  - StateKind { Surface, Treeview }.
  - StandardRequest::Damage(kind) replaces DamageSurface/DamageTreeview.
  - StandardEvent::DamageAck(kind) replaces the two old ack events.
  - New WlSurfaceEvent::Close.
- raw_client_initializer.rs: the trait now takes a request type (StandardRequest by default), and init gets the two SharedStates, including a new treeview argument.

sar
- Deleted its own client_handle.rs; the runner uses the common one.
- Ignores Damage(Treeview), since the root draws its treeview into its surface.
- Handles WlSurfaceEvent::Close by sending xdg close to the client. Added a small toplevel_of helper.
- Ported examples/run_basic_client.rs.

sttk
- Deleted: basic_applet.rs, caching_applet.rs, wl_surface_applet.rs.
- nodular_applet/mod.rs:
  - StandardApplet now has just three methods: handle an event, draw the window, give the treeview.
  - AppletContext replaces the hooks and keeps the same method names. Damage goes through a channel to the applet's own loop, so it works from any thread. It also gives direct access to the shared spawner registry.
  - NodeRequest covers Standard, AddChild, ChangeFocus and Close.
  - AppletClient runs each applet on its own thread and publishes its surface and treeview.
- recursive_node_applet.rs: RecursiveNode replaces RecursiveNodeApplet.
  - A node holds a main (an applet or a wl surface), its children, a focus index and a NodeId (an atomic counter).
  - NodeEnv gives nodes a way to spawn applets, the wl event queue, a list of requests nodes make themselves, and a damaged flag.
  - The focus logic is ported unchanged, and requests from below bubble up to ancestors.
  - Ctrl+W on a wl tile asks the client to close. The tile goes away when the client destroys its toplevel.
- root_node_applet.rs:
  - Holds the tree, routes each applet's requests by NodeId, and handles wl registration, focus and destruction.
  - Takes a plain initializer for the first applet.
  - Removed the commented-out NodularHolderApplet block.
- standard_keybinds.rs: a new standard_keybind_request builds the request for a keybind. It's shared between applets (handle_standard_keybinds) and wl tiles.
- creatable_applet.rs: uses AppletContext, and its arguments must be Send + 'static.
- command_hub: ported. find_applet_spawner now takes &str.

Standard tabs and SDE
- Ported code_editor, image_viewer and one_char_render_test. Spawners no longer wrap applets in a node themselves.
- singularity_sde passes CommandHubApplet::get_boxed_initiator() straight to the root.
```

The purpose of it was to enable pluck and place, which I envisioned via transfering ownership.
(This is relevant for plucking and placing certain elements (a part of the inner tree), if allowed.
If an element is focused and the user tries to pluck, then the applet in charge of it
can either return some child applet if applicable like an editor embedded within a file explorer,
or none in which case the whole applet is picked up.
Iow, it allows for pluck and place to happen recursively.)

A big change in philosophy is that I have finally decided to make it so that
an applet only controls/owns it's inner tree things (which, reminder, is it's own value in a world tree),
and no longer owns and manages its children nodes.
An applet can still request its owner to spawn children and stuff, but it is indirect.

2026-10-15 10:46PM

Here's my clearer plan for pluck and place:

When the server asks the root to pluck at some world tree index,
the root must return the closest pluckable tree of the requested element.
So a structural node that owns the applet requested element will ask the applet
for the plucked element.
If the inner applet says it has no pluckable elements,
then the structural node just plucks itself
(it returns the inner applet and also all the children).

On pluck, the plucked thing should be removed from the normal treeview,
and it should be held by the server in an optional plucked tree.

On place, it's a similar algorithm,
where the place propogates downwards and then if the bottom most applet can't place,
it calls spawn child upwards.
(Is this tuff?)

The shortcut is Alt+P for both operations, and I guess this will be the first global shortcut.

2026-10-15 11:40PM

I was going to let this run while sleeping, but it finished in 20 minutes (21m 11s to be exact):

```txt
❯ Use Alt+P for both pluck and place. This can be the first global keybind.

  Actually, I'll let you make the decisions and implement first, and just store every choice and major change you make or question for me into a document so you can
  summarize the changes later. When you're done with that, you have freedom to change how the tree is displayed, make it look nice. Then, do click to focus on tree, then
  mouse drag for pluck and place. Update the treeview UI to make it react better to these mouse operations.

  Don't commit or make new branches, because that requires my password.

  If it helps, you can rewrite the .claude/TODO.md to keep track.
```

it's own summary:

```txt
Pluck and place work end to end: Alt+P, click-to-focus in the treeview, and drag to pluck and place. I didn't commit anything. Every decision, open question and issue is logged in .claude/PLUCK_PLACE.md.

The workspace builds and clippy has nothing new in the touched files. A throwaway headless test (now deleted) drove the root through a fake server and everything passed:
- Plucking a focused node, and placing it back.
- Dragging a node that has a child and dropping it on another row.
- Re-attached applets still work: renaming shows up, and Ctrl+W closes the right node.
- Trying to pluck the root does nothing.

Two short live runs of the app had no panics, and screenshots show the new treeview. I couldn't send input to the live window, so Alt+P, the drag preview, hover highlighting and plucking Wayland windows haven't been tested by hand. Those are the things to try.

How it works:
- Moving clients between owners: a client can now be taken out of its owner's event loop and attached to another one, with its thread still running. Requests it sends while unattached wait and are handled once it's placed. The server (sar) holds the plucked tree without knowing what kind of requests its clients send.
- Pluck: the root asks the target node's applet first. If the applet says it has nothing to pluck (every current applet does), the node plucks itself with its children. The root can't be plucked. When the node being plucked is the one that just answered, the removal waits until after the current batch of events, because calloop panics otherwise.
- Place: the tree goes to the closest existing node along the path. That node's applet is asked first; if it declines, the tree becomes the node's last child and gets focus.
- Alt+P is handled in sar: it plucks at the current focus, or places there if something is already plucked.
- Treeview: one indented outline replaces the per-level columns, with guide lines and a marker on the focused row. The marker is yellow when the node itself is focused and green when focus is inside its applet. Hovered rows highlight, the dragged row dims, the drop target gets an outline, and what's plucked follows the pointer in a small box.
- Fast drags: if you release before the pluck finishes, the drop's position could point at the wrong node once the tree shifts. The root remembers which node the drop was on and uses its current position instead.

Smaller changes:
- StandardRequest is no longer Copy.
- ClientHandle is no longer Send; nothing needed it.
- I removed the root's debug print that ran on every redraw.

Questions for you (also in the doc):
1. Should dropping a node make it a sibling of the target instead of its last child?
2. A Wayland window that closes while plucked stays in the plucked tree and gets placed back as a dead tile.
3. If an applet never answers a pluck or place (for example it panicked), the root and server wait forever.
4. Should a placed tree keep its old inner focus? Right now it resets.

Issues found:
- Colors: background colors are drawn as linear while text colors aren't, so #121418 shows as about #4b5059. I pre-darkened the treeview colors to compensate; the real fix belongs in the renderer.
- 1px lines: a 1px background rect drew as a bright line, so I dropped the separator between the treeview and the applet.
- todo!(): the command hub panics on FocusChanged, so the root can never send that event.

For CLAUDE.md: "moving nodes between owners (pluck and place)" can come off the not-done list, replaced by questions 2–3 above and the fact that no applet supports plucking from inside itself yet. The doc lists the other notes worth adding.

On my process: one edit went through sed, against your saved preference. It was a two-line change to eprintln! in singularity_sar/src/pluck_place.rs plus the module line in singularity_sar/src/lib.rs; everything else went through Edit. To screenshot the window I used niri, which copies to the clipboard. I saved your clipboard text first and restored it afterwards.
```

I'll commit now and verify this later (if I remember).
It runs, and it seems to work.

I want to give it a harder task...

I'm going to say this:

```txt
Make a file manager that, when I click/open a file, it opens the file in an editable way with the editor file embedded.
This should be embedded, so the editor UI shows up inside the file manager UI (along with the file tree) and is a node in the inner tree.
I want to do this to test plucking from elements.

Make splitting panes. To start, just do 2-way splits.

Make it so I can run Sonamu as an actual compositor (smithay udev backend?).

Once you add these features, you may start looking for and squashing bugs and resolving TODOs.
If that is done, improve the features you added.
Try to make a new session with a fresh context if you can when you are at this stage,
so that the context window doesn't fill up.

Make the decisions and implement first, and just store every choice and major change you make or question for me into a document so you can summarize the changes later. When you're done with that, you have freedom to change how the tree is displayed, make it look nice.
Don't commit or make new branches, because that requires my password.
If it helps, you can rewrite the .claude/TODO.md to keep track.
```

2026-10-16 12:30AM

I thought of more things:

```txt
Once the general purpose subagent finishes, make a new agent with similar instructions but these new tasks:

Redo the command hub to have the features it currently has, but make it look good and possibly have more features.

Make it so WL applets spawn under the focused one, use a downwards propagation then upwards add_child maybe.

Add a help applet that appears on Super+?.

Make it so that the tree view only appears during traversal (make it a setting with a const in the code for now).

Do some type of project management, where a project is a node and it's inner tree acts as if it was a root. Let the user set where the view root is, so when they are
working within the context of a project, then during traversal it looks like the view root is the outermost tree. But of course, the user can widen the view root if they
keep going out.
```

2026-10-16 06:18AM

My running mate is getting ready so I ran Sonamu.

2026-10-16 09:47AM

I'm back, wow, it's made a lot of progress.
The first part took 20 min and the rest (cleaning subagent + second batch of features) took 53 min.

It didn't quite do projects how I wanted it to, but this is still very impressive.

I'll commit then test running this with udev.

2026-10-16 10:03AM

Ok, I'm in udev > Sonamu > wayland applet > alacritty > helix.
Main point, udev works!

It is going to be slightly weird to develop sonamu in sonamu,
because it doesn't do hot reloading and when I spawn it embedded,
the keybinds might get intercepted...

Actually, it just ERR's.
This isn't really a problem though and unless it is caused by a deeper issue that will cause more bugs,
I don't really care.

The first thing I'll fix is the cursor is just a rectangle and that I want to fix the scroll direction.

Ok, it fixed the cursor and I can just test it in a new TTY
(I was afraid multiple sonamu via TTY would crash, but of course Claude mitigated that).

I'm also going to stop saying "(claude) ..." for the commits,
that will be the new default.

2026-10-16 01:39PM

I am genuinely reassessing the value of handcoding.

Either ways, it is clear that I should embrace AI.

I want a workflow where throughout the day, I can just put ideas into GitHub issues on my phone.
Then, Claude sees an issue, implements it, then makes a PR.

I'm going to do this with [Github Actions](https://code.claude.com/docs/en/github-actions),
which lets me just `@claude` in GitHub issues and apparently it does stuff on the cloud.

2026-10-18 09:54PM

I haven't been documenting in DEVLOG ever since I started vibecoding Sonamu.

Well, this is going to be the first commit to the repo where it's named Sonamu.

I'm daily driving Sonamu, I am going to tag this as the MVP,
and I just pushed the 100th thing to the repo.

I would usually take thiis time to get philosophical,
but I haven't much time because of work.
I think that is emblematic of my life now.
But it's a sacrifice I'm willing to make,
trading the small things for the big.

I have been feeling that I am destined to make a new path for myself,
grant me courage.

2026-10-27 01:22PM

Took a bunch of screenshots with grim and swappy.
But swappy sucks and didn't actually save any of them.
