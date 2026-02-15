// TempleLinux wrapper for the TempleOS wallpaper-controls demo.
//
// Upstream file: ::/Demo/Graphics/WallPaperCtrl.HC
// Notes:
// - The upstream demo is intended to be "Adam Included" (SHIFT-F5 in TempleOS).
// - TempleLinux doesn't emulate full Adam/include lifetimes yet, so we run it as a normal app
//   and keep it alive with a simple draw loop so the control is visible and interactive.

U0 TLSetup()
{
  // Satisfy the upstream "Adam Included" guard and keep semantics simple for now.
  // (In TempleOS, `adam_task` is a persistent task and `sys_winmgr_task` is the window manager.)
  adam_task=Fs;
  sys_winmgr_task=Fs;
}

TLSetup;

#include "::/Demo/Graphics/WallPaperCtrl.HC"

U0 Draw(CTask *task,CDC *dc)
{
  Clear(BLACK);
  dc->color=WHITE;
  GrPrint(dc,8,8,
    "WallPaperCtrl (TempleOS demo) - click/drag sliders - Esc exits");
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
