mod commands;
pub mod event;
pub mod state;
mod update;

pub use state::App;

pub fn print_keys() {
    println!("git-review-tui key bindings");
    println!("===========================");
    println!();
    println!("Navigation:");
    println!("  j/Down     Move down");
    println!("  k/Up       Move up");
    println!("  g/Home     Go to top");
    println!("  G/End      Go to bottom");
    println!("  PgUp       Page up");
    println!("  PgDn       Page down");
    println!("  Tab        Switch focus (files <-> diff)");
    println!();
    println!("Diff:");
    println!("  n          Next hunk");
    println!("  p          Previous hunk");
    println!("  f          Search in diff");
    println!();
    println!("Mode:");
    println!("  m          Toggle Worktree <-> Compare");
    println!("  s          Toggle unstaged <-> staged (Worktree)");
    println!();
    println!("Selectors:");
    println!("  b          Base selector (Compare mode)");
    println!("  t          Target selector (Compare mode)");
    println!("  w          Worktree selector");
    println!("  c          Context selector (repo/worktree switch)");
    println!();
    println!("Output:");
    println!("  y          Copy current hunk");
    println!("  Y          Copy file diff");
    println!("  o          Export diff to file");
    println!();
    println!("Other:");
    println!("  r          Reload");
    println!("  ?          Help");
    println!("  q/Esc      Quit (or close overlay)");
}
