use crate::nodular_applet::{AppletContext, NodularAppletInitializer, StandardApplet};

pub trait CreatableNodularApplet<Args>: StandardApplet + Sized
where
    // idk why this is needed, something something box I think
    Self: 'static,
    // the applet is made on its own thread
    Args: Send + 'static,
{
    fn new(args: Args, hook: AppletContext) -> Self;

    fn get_initiator(args: Args) -> impl FnOnce(AppletContext) -> Self {
        |hook: AppletContext| Self::new(args, hook)
    }
    fn get_boxed_initiator(args: Args) -> NodularAppletInitializer {
        Box::new(|hook: AppletContext| Box::new(Self::new(args, hook)))
    }
    // fn get_applet_spawner() -> AppletSpawner {
    //     struct EditorSpawner;
    //     impl AppletSpawnerTrait for EditorSpawner {
    //         fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
    //             let file_path = args.first()?;

    //             Some(RecursiveNodeApplet::boxed_get_boxed_initializer(
    //                 TextEditorApplet::get_boxed_initiator(file_path.to_string()),
    //             ))
    //         }

    //         fn duplicate(&self) -> AppletSpawner {
    //             Box::new(Self)
    //         }
    //     }
    //     Box::new(EditorSpawner)
    // }
}
