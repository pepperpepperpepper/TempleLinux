// TempleLinux: TimeClock wrapper app (interactive UI for upstream ::/Apps/TimeClock).
//
// TempleOS TimeClock is designed to be loaded and then used via commands like:
//   PunchIn;
//   PunchOut;
//   TimeRep;
//
// TempleLinux runs HolyC programs as apps, so this wrapper provides a small
// interactive menu while reusing the unmodified upstream TimeClock sources.

#include "::/Apps/TimeClock/Load.HC"

U0 Main()
{
  I64 k;

  while (TRUE) {
    DocClear;
    "$$LTGREEN$$TimeClock$$FG$$\n\n";
    "  $$LTGREEN$$I$$FG$$ : PunchIn\n";
    "  $$LTGREEN$$O$$FG$$ : PunchOut\n";
    "  $$LTGREEN$$R$$FG$$ : Report\n";
    "  $$LTGREEN$$Esc$$FG$$ : Exit\n\n";
    "Choice:";

    k=PressAKey();
    if (k==CH_ESC || k==CH_SHIFT_ESC)
      break;

    if (k=='i' || k=='I') {
      PunchIn;
      "\nPress any key...\n";
      PressAKey;
    } else if (k=='o' || k=='O') {
      PunchOut;
      "\nPress any key...\n";
      PressAKey;
    } else if (k=='r' || k=='R') {
      DocClear;
      TimeRep(NULL);
      "\nPress any key...\n";
      PressAKey;
    }
  }
}

Main;

