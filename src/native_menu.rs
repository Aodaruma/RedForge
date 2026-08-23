use bevy::prelude::*;

/// An application command selected from the native menu bar.
///
/// Keyboard shortcuts should emit the same command (or call the same handler)
/// from `app.rs`, so menu and keyboard behavior cannot drift apart.
#[derive(Clone, Copy, Debug, Eq, Message, PartialEq)]
pub enum MenuAction {
    New,
    Open,
    Save,
    SaveAs,
    Exit,
    Undo,
    Redo,
    Copy,
    Paste,
    Delete,
    Rotate,
    Paint,
    Select,
    Top,
    Isometric,
    Orbit,
    LayerDown,
    LayerUp,
    Focus,
    Start,
    Use,
    Step,
    RunPause,
    Reset,
}

impl MenuAction {
    const fn id(self) -> &'static str {
        match self {
            Self::New => "redforge.file.new",
            Self::Open => "redforge.file.open",
            Self::Save => "redforge.file.save",
            Self::SaveAs => "redforge.file.save-as",
            Self::Exit => "redforge.file.exit",
            Self::Undo => "redforge.edit.undo",
            Self::Redo => "redforge.edit.redo",
            Self::Copy => "redforge.edit.copy",
            Self::Paste => "redforge.edit.paste",
            Self::Delete => "redforge.edit.delete",
            Self::Rotate => "redforge.edit.rotate",
            Self::Paint => "redforge.edit.paint",
            Self::Select => "redforge.edit.select",
            Self::Top => "redforge.view.top",
            Self::Isometric => "redforge.view.isometric",
            Self::Orbit => "redforge.view.orbit",
            Self::LayerDown => "redforge.view.layer-down",
            Self::LayerUp => "redforge.view.layer-up",
            Self::Focus => "redforge.view.focus",
            Self::Start => "redforge.simulation.start",
            Self::Use => "redforge.simulation.use",
            Self::Step => "redforge.simulation.step",
            Self::RunPause => "redforge.simulation.run-pause",
            Self::Reset => "redforge.simulation.reset",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "redforge.file.new" => Self::New,
            "redforge.file.open" => Self::Open,
            "redforge.file.save" => Self::Save,
            "redforge.file.save-as" => Self::SaveAs,
            "redforge.file.exit" => Self::Exit,
            "redforge.edit.undo" => Self::Undo,
            "redforge.edit.redo" => Self::Redo,
            "redforge.edit.copy" => Self::Copy,
            "redforge.edit.paste" => Self::Paste,
            "redforge.edit.delete" => Self::Delete,
            "redforge.edit.rotate" => Self::Rotate,
            "redforge.edit.paint" => Self::Paint,
            "redforge.edit.select" => Self::Select,
            "redforge.view.top" => Self::Top,
            "redforge.view.isometric" => Self::Isometric,
            "redforge.view.orbit" => Self::Orbit,
            "redforge.view.layer-down" => Self::LayerDown,
            "redforge.view.layer-up" => Self::LayerUp,
            "redforge.view.focus" => Self::Focus,
            "redforge.simulation.start" => Self::Start,
            "redforge.simulation.use" => Self::Use,
            "redforge.simulation.step" => Self::Step,
            "redforge.simulation.run-pause" => Self::RunPause,
            "redforge.simulation.reset" => Self::Reset,
            _ => return None,
        })
    }
}

/// Adds the native command message on every platform and a `muda` menu bar on
/// Windows. The no-op menu on other platforms keeps the rest of the app free
/// of platform `cfg` branches.
pub struct NativeMenuPlugin;

impl Plugin for NativeMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MenuAction>();

        #[cfg(target_os = "windows")]
        windows::install(app);
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use bevy::prelude::*;
    use bevy::window::{PrimaryWindow, RawHandleWrapper};
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use raw_window_handle::RawWindowHandle;

    use super::MenuAction;

    struct NativeMenu {
        menu: Menu,
        attached_hwnd: Option<isize>,
    }

    impl NativeMenu {
        fn new() -> muda::Result<Self> {
            let new = item(MenuAction::New, "&New\tCtrl+N");
            let open = item(MenuAction::Open, "&Open...\tCtrl+O");
            let save = item(MenuAction::Save, "&Save\tCtrl+S");
            let save_as = item(MenuAction::SaveAs, "Save &As...\tCtrl+Shift+S");
            let exit = item(MenuAction::Exit, "E&xit\tAlt+F4");
            let file_separator = PredefinedMenuItem::separator();
            let file = Submenu::with_items(
                "&File",
                true,
                &[&new, &open, &save, &save_as, &file_separator, &exit],
            )?;

            let undo = item(MenuAction::Undo, "&Undo\tCtrl+Z");
            let redo = item(MenuAction::Redo, "&Redo\tCtrl+Shift+Z");
            let history_separator = PredefinedMenuItem::separator();
            let copy = item(MenuAction::Copy, "&Copy\tCtrl+C");
            let paste = item(MenuAction::Paste, "&Paste\tCtrl+V");
            let delete = item(MenuAction::Delete, "&Delete\tDelete");
            let tools_separator = PredefinedMenuItem::separator();
            let paint = item(MenuAction::Paint, "P&aint Tool");
            let select = item(MenuAction::Select, "&Select Tool\tV");
            let rotate = item(MenuAction::Rotate, "Rotate &Facing\tR");
            let edit = Submenu::with_items(
                "&Edit",
                true,
                &[
                    &undo,
                    &redo,
                    &history_separator,
                    &copy,
                    &paste,
                    &delete,
                    &tools_separator,
                    &paint,
                    &select,
                    &rotate,
                ],
            )?;

            let top = item(MenuAction::Top, "&Top\t1");
            let isometric = item(MenuAction::Isometric, "&Isometric\t2");
            let orbit = item(MenuAction::Orbit, "&Orbit\t3");
            let camera_separator = PredefinedMenuItem::separator();
            let layer_down = item(MenuAction::LayerDown, "Layer &Down\t[");
            let layer_up = item(MenuAction::LayerUp, "Layer &Up\t]");
            let focus = item(MenuAction::Focus, "&Focus Selection\tF");
            let view = Submenu::with_items(
                "&View",
                true,
                &[
                    &top,
                    &isometric,
                    &orbit,
                    &camera_separator,
                    &layer_down,
                    &layer_up,
                    &focus,
                ],
            )?;

            let start = item(MenuAction::Start, "&Start");
            let use_block = item(MenuAction::Use, "&Use Selected Block");
            let step = item(MenuAction::Step, "S&tep\t.");
            let run_pause = item(MenuAction::RunPause, "&Run / Pause\tSpace");
            let reset = item(MenuAction::Reset, "&Reset");
            let simulation = Submenu::with_items(
                "&Simulation",
                true,
                &[&start, &use_block, &step, &run_pause, &reset],
            )?;

            Ok(Self {
                menu: Menu::with_items(&[&file, &edit, &view, &simulation])?,
                attached_hwnd: None,
            })
        }
    }

    fn item(action: MenuAction, text: &str) -> MenuItem {
        // The shortcut text is intentionally visual only. Bevy owns keyboard
        // input; muda's Win32 accelerators require a custom winit message loop.
        MenuItem::with_id(action.id(), text, true, None)
    }

    pub(super) fn install(app: &mut App) {
        match NativeMenu::new() {
            Ok(menu) => {
                app.insert_non_send(menu)
                    .add_systems(First, (attach_native_menu, forward_native_menu_events));
            }
            Err(error) => eprintln!("native menu could not be created: {error}"),
        }
    }

    fn attach_native_menu(
        mut native_menu: NonSendMut<NativeMenu>,
        windows: Query<&RawHandleWrapper, With<PrimaryWindow>>,
    ) {
        if native_menu.attached_hwnd.is_some() {
            return;
        }

        let Ok(raw_handle) = windows.single() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = raw_handle.get_window_handle() else {
            return;
        };
        let hwnd = handle.hwnd.get();

        // SAFETY: Bevy's RawHandleWrapper belongs to the live primary window,
        // and this First-schedule system runs on the winit/main thread because
        // it accesses a NonSend resource. NativeMenu owns the Menu until exit.
        match unsafe { native_menu.menu.init_for_hwnd(hwnd) } {
            Ok(()) => native_menu.attached_hwnd = Some(hwnd),
            Err(error) => eprintln!("native menu could not be attached: {error}"),
        }
    }

    fn forward_native_menu_events(mut actions: MessageWriter<MenuAction>) {
        for event in MenuEvent::receiver().try_iter() {
            if let Some(action) = MenuAction::from_id(event.id.as_ref()) {
                actions.write(action);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MenuAction;

    #[test]
    fn every_menu_action_id_round_trips() {
        let actions = [
            MenuAction::New,
            MenuAction::Open,
            MenuAction::Save,
            MenuAction::SaveAs,
            MenuAction::Exit,
            MenuAction::Undo,
            MenuAction::Redo,
            MenuAction::Copy,
            MenuAction::Paste,
            MenuAction::Delete,
            MenuAction::Rotate,
            MenuAction::Paint,
            MenuAction::Select,
            MenuAction::Top,
            MenuAction::Isometric,
            MenuAction::Orbit,
            MenuAction::LayerDown,
            MenuAction::LayerUp,
            MenuAction::Focus,
            MenuAction::Start,
            MenuAction::Use,
            MenuAction::Step,
            MenuAction::RunPause,
            MenuAction::Reset,
        ];

        for action in actions {
            assert_eq!(MenuAction::from_id(action.id()), Some(action));
        }
    }
}
