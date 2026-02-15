// TempleLinux wrapper for the TempleOS wallpaper fish demo.
//
// Upstream file: ::/Demo/Graphics/WallPaperFish.HC
// Notes:
// - The upstream demo is intended to be "Adam Included" (SHIFT-F5 in TempleOS) so the wallpaper
//   routine stays resident.
// - TempleLinux doesn't emulate Adam include lifetimes yet, so we run it as a normal app and keep
//   it alive with a simple draw loop.

U0 TLSetup()
{
  // Satisfy the upstream "Adam Included" guard and keep semantics simple for now.
  // (In TempleOS, `adam_task` is a persistent task and `sys_winmgr_task` is the window manager.)
  adam_task=Fs;
  sys_winmgr_task=Fs;
}

TLSetup;

#include "::/Demo/Graphics/WallPaperFish.HC"

U0 Draw(CTask *task,CDC *dc)
{
  Clear(BLACK);
  WallPaperFish(task);
  dc->color=WHITE;
  GrPrint(dc,8,8,"WallPaperFish (TempleOS demo) - Esc exits");
}

U0 Main()
{
  Fs->draw_it=&Draw;
  while (TRUE) {
    Sleep(16);
    I64 k=ScanChar;
    if (k==CH_ESC || k==CH_SHIFT_ESC)
      return;
  }
}

Main;

