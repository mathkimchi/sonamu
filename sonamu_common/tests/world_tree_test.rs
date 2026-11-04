use sonamu_common::utils::tree::{
    recursive_tree::RecursiveTreeNode,
    tree_node_path::TreeNodePath,
    world_tree::{Step, WorldTree, WorldTreePath},
};

#[test]
fn test_world_tree() {
    let a = WorldTree::Base("Hi");
    assert_eq!(a.get_root_value(), &"Hi");
    assert!(a.safe_get(&WorldTreePath::root()).is_none());

    let b = WorldTree::new_world(a);
    assert_eq!(b.get_root_value(), &"Hi");
    assert!(b.safe_get(&WorldTreePath::root()).unwrap().get_value() == &WorldTree::Base("Hi"));
    assert!(b.safe_get(&WorldTreePath(vec![Step::Child(0)])).is_none());
    assert!(b.safe_get(&WorldTreePath(vec![Step::Inner(0)])).is_none());
}

#[test]
fn test_paths() {
    let path = WorldTreePath(vec![
        Step::Child(2),
        Step::Child(1),
        Step::Inner(0),
        Step::Child(1),
    ]);
    let (node, inner) = path.split_world();
    assert_eq!(node, TreeNodePath(vec![2, 1]));
    let inner = inner.unwrap();
    assert_eq!(inner, WorldTreePath(vec![Step::Child(0), Step::Child(1)]));
    assert_eq!(WorldTreePath::join(&node, &inner), path);

    // a node and its inner root are the same
    assert_eq!(
        WorldTreePath::join(&node, &WorldTreePath::root()),
        WorldTreePath::from_tree_path(&node)
    );
    assert_eq!(WorldTreePath::from_tree_path(&node).split_world().1, None);
}

#[test]
fn test_printing() {
    /*
     * From the DEVLOG 2026-02-19 10:56AM:
     *
     *
     * // Just a value (level 0)
     * ( [] )
     *
     * // Structure of a normal tree (level 1)
     * ( [[]] )
     * |
     * |--- ( [[0]] )
     * |
     * |--- ( [[1]] )
     * |
     * |--- ( [[2]] )
     *          |
     *          |--- ( [[2, 0]] )
     *          |
     *          |--- ( [[2, 1]] )
     *
     * // Level 2: tree in a tree
     * // Now imagine that the above tree is still the outermost tree,
     * // but replace ([[2, 1]]) with the following tree:
     * ...
     * ( [[2, 1], []] )
     * |
     * |--- ( [[2, 1], [0]] )
     * |           |
     * |           |--- ( [[2, 1], [0, 0]] )
     * |           |
     * |           |--- ( [[2, 1], [0, 1]] )
     * |
     * |--- ( [[2, 1], [1]] )
     *             |
     *             |--- ( [[2, 1], [1, 0]] )
     *             |
     *             |--- ( [[2, 1], [1, 1]] )
     */
    let level_0 = WorldTree::Base("([])".to_string());
    println!("Level 0:");
    println!("{}", level_0.outer_world_to_string(None));

    let level_1 = WorldTree::World(Box::new(RecursiveTreeNode::new(
        WorldTree::Base("([[]])".to_string()),
        vec![
            RecursiveTreeNode::from_value(WorldTree::Base("([[0]])".to_string())),
            RecursiveTreeNode::from_value(WorldTree::Base("([[1]])".to_string())),
            RecursiveTreeNode::new(
                WorldTree::Base("([[2]])".to_string()),
                vec![
                    RecursiveTreeNode::from_value(WorldTree::Base("([[2, 0]])".to_string())),
                    RecursiveTreeNode::from_value(WorldTree::Base("([[2, 1]])".to_string())),
                ],
            ),
            RecursiveTreeNode::new(
                WorldTree::Base("([[3]])".to_string()),
                vec![
                    RecursiveTreeNode::from_value(WorldTree::Base("([[3, 0]])".to_string())),
                    RecursiveTreeNode::from_value(WorldTree::Base("([[3, 1]])".to_string())),
                ],
            ),
        ],
    )));
    println!("Level 1:");
    println!("{}", level_1.outer_world_to_string(None));

    let level_2 = WorldTree::World(Box::new(RecursiveTreeNode::new(
        WorldTree::Base("([[]])".to_string()),
        vec![
            RecursiveTreeNode::from_value(WorldTree::Base("([[0]])".to_string())),
            RecursiveTreeNode::from_value(WorldTree::Base("([[1]])".to_string())),
            RecursiveTreeNode::new(
                WorldTree::Base("([[2]])".to_string()),
                vec![
                    RecursiveTreeNode::from_value(WorldTree::Base("([[2, 0]])".to_string())),
                    RecursiveTreeNode::from_value(WorldTree::World(Box::new(
                        RecursiveTreeNode::new(
                            WorldTree::Base("([[2, 1], []])".to_string()),
                            vec![
                                RecursiveTreeNode::new(
                                    WorldTree::Base("([[2, 1], [0]])".to_string()),
                                    vec![
                                        RecursiveTreeNode::from_value(WorldTree::Base(
                                            "([[2, 1], [0, 0]])".to_string(),
                                        )),
                                        RecursiveTreeNode::from_value(WorldTree::Base(
                                            "([[2, 1], [0, 1]])".to_string(),
                                        )),
                                    ],
                                ),
                                RecursiveTreeNode::new(
                                    WorldTree::Base("([[2, 1], [1]])".to_string()),
                                    vec![
                                        RecursiveTreeNode::from_value(WorldTree::Base(
                                            "([[2, 1], [1, 0]])".to_string(),
                                        )),
                                        RecursiveTreeNode::from_value(WorldTree::Base(
                                            "([[2, 1], [1, 1]])".to_string(),
                                        )),
                                    ],
                                ),
                            ],
                        ),
                    ))),
                ],
            ),
            RecursiveTreeNode::new(
                WorldTree::Base("([[3]])".to_string()),
                vec![
                    RecursiveTreeNode::from_value(WorldTree::Base("([[3, 0]])".to_string())),
                    RecursiveTreeNode::from_value(WorldTree::Base("([[3, 1]])".to_string())),
                ],
            ),
        ],
    )));
    println!("Level 2 outer:");
    println!("{}", level_2.outer_world_to_string(None));
    println!("Level 2 indexed:");
    println!(
        "{}",
        level_2
            .safe_get(&WorldTreePath(vec![Step::Child(2), Step::Child(1)]))
            .unwrap()
            .get_value()
            .outer_world_to_string(None)
    );
}
