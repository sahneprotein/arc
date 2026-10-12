//! The system's folder chooser, for the project browser.
//!
//! The browser asks for a folder ([`Launcher::wants_folder`]); this shows the desktop's own
//! dialog for choosing one and hands the answer back. The dialog is the desktop's: it has
//! the person's places and recent folders, and it can make a new folder, which is how a new
//! project gets one of its own. While it is open the editor's window waits behind it.
//!
//! Where there is no desktop to show a dialog on and nobody to answer it, as in a scripted
//! check, the application has the chooser answer from a list instead
//! ([`FolderChooser::scripted`]): the next folder that was named for it, and until one is
//! named the question stays open. No dialog is shown then.

use std::{collections::VecDeque, path::PathBuf};

use bevy::{ecs::system::NonSendMarker, prelude::*};

use crate::launcher::{FolderFor, Launcher};

/// How the browser's question for a folder is answered: by the person, in the desktop's
/// folder chooser, or from the folders a script named.
#[derive(Resource, Default)]
pub struct FolderChooser {
    /// The answers a script named, in order, when it is a script that answers. `None` in
    /// the list is the person choosing no folder.
    scripted: Option<VecDeque<Option<PathBuf>>>,
}

impl FolderChooser {
    /// A chooser that shows no dialog and answers with what is named for it.
    pub fn scripted() -> Self {
        Self {
            scripted: Some(VecDeque::new()),
        }
    }

    /// Whether it is a script that answers, and no dialog is shown.
    pub fn is_scripted(&self) -> bool {
        self.scripted.is_some()
    }

    /// Names what a scripted chooser answers the next time it is asked: a folder, or `None`
    /// for choosing none. Whether it was taken: a chooser that shows the desktop's dialog
    /// takes no answers.
    pub fn answer_next(&mut self, folder: Option<PathBuf>) -> bool {
        match &mut self.scripted {
            Some(answers) => {
                answers.push_back(folder);
                true
            }
            None => false,
        }
    }
}

/// What the dialog is called, by what the folder is chosen for.
pub fn dialog_title(why: FolderFor) -> &'static str {
    match why {
        FolderFor::Open => "Open Project: choose the project's folder",
        FolderFor::New => "New Project: choose or make the folder for the project",
    }
}

/// Answers the browser when it asks for a folder: with the desktop's folder chooser, or
/// with the next answer a script named.
///
/// The dialog is shown from the application's main thread, as the desktop requires of it,
/// and this returns when the person has chosen or has closed it.
pub(crate) fn choose_folder(
    mut launcher: ResMut<Launcher>,
    mut chooser: ResMut<FolderChooser>,
    _main_thread: NonSendMarker,
) {
    let Some(why) = launcher.wants_folder() else {
        return;
    };
    if let Some(answers) = &mut chooser.scripted {
        if let Some(answer) = answers.pop_front() {
            launcher.folder_chosen(answer);
        }
        return;
    }
    let chosen = rfd::FileDialog::new()
        .set_title(dialog_title(why))
        .set_can_create_directories(true)
        .pick_folder();
    launcher.folder_chosen(chosen);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_says_what_the_folder_is_chosen_for() {
        assert!(dialog_title(FolderFor::Open).starts_with("Open Project"));
        let new = dialog_title(FolderFor::New);
        assert!(new.starts_with("New Project") && new.contains("make the folder"));
    }

    #[test]
    fn only_a_scripted_chooser_takes_answers_and_it_shows_no_dialog() {
        let mut desktop = FolderChooser::default();
        assert!(!desktop.is_scripted());
        assert!(!desktop.answer_next(Some(PathBuf::from("/projects/harbour"))));
        let mut scripted = FolderChooser::scripted();
        assert!(scripted.is_scripted());
        assert!(scripted.answer_next(Some(PathBuf::from("/projects/harbour"))));
        assert!(scripted.answer_next(None));
        assert_eq!(scripted.scripted.as_ref().map(VecDeque::len), Some(2));
    }
}
