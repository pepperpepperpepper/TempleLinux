// TempleLinux: ClipboardBridge — host clipboard helper.
//
// TempleShell already supports:
//   - Ctrl+V / Shift+Ins paste from the host clipboard into the focused app
//   - ClipPutS("text") from HolyC apps to set the host clipboard
//
// This app is a small TempleOS-ish UI that makes that flow easy:
// - Paste into a multi-line GetStr prompt to populate a buffer.
// - Copy the buffer to the host clipboard via ClipPutS.

U8 *g_buf="";
U8 *g_status="";

U0 Draw()
{
  DocClear;
  "$$LTGREEN$$ClipboardBridge$$FG$$\n\n";
  "  $$LTGREEN$$E$$FG$$ : Edit buffer (Ctrl+V / Shift+Ins to paste)\n";
  "  $$LTGREEN$$C$$FG$$ : Copy buffer to host clipboard\n";
  "  $$LTGREEN$$X$$FG$$ : Clear buffer + clipboard\n";
  "  $$LTGREEN$$Esc$$FG$$ : Exit\n\n";

  "$$LTGREEN$$Status$$FG$$: %s\n\n", g_status;
  "$$LTGREEN$$Buffer$$FG$$ (preview):\n";
  "%s\n", g_buf;
}

U0 EditBuf()
{
  U8 *st;
  st=GetStr("Clipboard:", g_buf, 2);
  if (st) {
    g_buf=st;
    g_status="Buffer updated (press C to copy)";
  } else {
    g_status="Edit canceled";
  }
}

U0 CopyBuf()
{
  ClipPutS(g_buf);
  g_status="Host clipboard set";
}

U0 ClearAll()
{
  g_buf="";
  ClipPutS("");
  g_status="Cleared";
}

U0 Main()
{
  I64 k;

  g_status="Ready";

  while (TRUE) {
    Draw;
    k=PressAKey();
    if (k==CH_ESC || k==CH_SHIFT_ESC)
      break;
    if (k=='e' || k=='E') {
      EditBuf;
    } else if (k=='c' || k=='C') {
      CopyBuf;
    } else if (k=='x' || k=='X') {
      ClearAll;
    }
  }
}

Main;
