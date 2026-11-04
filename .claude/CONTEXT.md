# Sonamu

A compositor that arranges native applets and Wayland clients, and their elements, into one recursive tree.

## Language

### Trees

**Outer Tree**:
The part of the hierarchy held by the root client (`RootNodeApplet`), from the root down to the nodes where an applet takes over management of its own inner tree.
_Avoid_: applet tree

**Inner Tree**:
A tree an applet owns and manages directly on its own thread; its node sees it only as a published treeview.

**Node**:
An outer-tree element with a main applet (a wl tile's main is a `WlSurfaceApplet`) and children. It owns its children; its main changes them only by request.
_Avoid_: node applet

**Server Container**:
An outer-tree node whose layout and logic run in the tree's holder rather than in an app (e.g. split view).
_Avoid_: container applet

### Input

**Interception**:
Claiming an event before it reaches the focused node. Mostly done by the server; an applet may intercept only if it has been explicitly granted permission.
_Avoid_: hook, filter
