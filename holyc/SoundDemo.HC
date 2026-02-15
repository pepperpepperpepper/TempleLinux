// TempleLinux SoundDemo — tiny sound showcase app.
//
// Keys:
// - 1..8  play notes
// - Esc   exit
U0 SoundDemo()
{
  I64 ch;

  "TempleLinux SoundDemo\n";
  "Press 1..8 to play notes. ESC exits.\n\n";

  while (TRUE) {
    ch=GetChar(,FALSE); // no echo
    if (ch==CH_ESC) break;
    if (ch==CH_SHIFT_ESC) break;

    if (ch=='1') { Snd(48); Sleep(120); Snd; continue; }
    if (ch=='2') { Snd(50); Sleep(120); Snd; continue; }
    if (ch=='3') { Snd(52); Sleep(120); Snd; continue; }
    if (ch=='4') { Snd(53); Sleep(120); Snd; continue; }
    if (ch=='5') { Snd(55); Sleep(120); Snd; continue; }
    if (ch=='6') { Snd(57); Sleep(120); Snd; continue; }
    if (ch=='7') { Snd(59); Sleep(120); Snd; continue; }
    if (ch=='8') { Snd(60); Sleep(180); Snd; continue; }

    // Small error chirp for unknown keys.
    Snd(72); Sleep(30); Snd;
  }

  Snd;
  "\n";
}

SoundDemo;
