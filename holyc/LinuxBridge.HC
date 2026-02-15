// TempleLinux: LinuxBridge — host integration from a TempleOS-style UI.
//
// This app is intentionally TempleOS-ish:
// - pull-down menu (MenuPush)
// - message loop (GetMsg)
// - crisp 8x8 text UI (Text/FillRect)
//
// TempleLinux-specific built-ins used:
//   LinuxBrowse("url")   -> pid or 0
//   LinuxOpen("path")    -> pid or 0
//   LinuxRun("cmd ...")  -> pid or 0  (deny-by-default allowlist)
//   LinuxLastErr()       -> string
//   GetStr(msg,dft,flags)-> string

#define CMD_BROWSE  1
#define CMD_OPEN    2
#define CMD_RUN     3
#define CMD_HELP    4
#define CMD_EXIT    5

U8 *g_url="https://";
U8 *g_path="/Home/";
U8 *g_cmd="google-chrome";
U8 *g_status="";
I64 g_focus=0; // 0=url 1=path 2=cmd

U0 DrawUI()
{
  I64 fg=WHITE,bg=BLACK;
  I64 x0=16;
  I64 y0=24;
  I64 w=SCR_W-32;

  Clear(BLACK);

  // Title bar (within the app window; TempleShell draws the outer chrome).
  FillRect(0,0,SCR_W,8,BLUE);
  Text(8,0,WHITE,BLUE,"LinuxBridge");

  Text(x0,y0,LTGREEN,bg,"Tab:Next  Enter:Go  Esc:Exit");
  Text(x0,y0+8,LTCYAN,bg,"Menu: Linux { Browse / Open / Run }  Help { Help }  File { Exit }");

  // Fields.
  Text(x0,y0+32,WHITE,bg,"URL:");
  Text(x0,y0+56,WHITE,bg,"Path:");
  Text(x0,y0+80,WHITE,bg,"Cmd:");

  I64 box_x=x0+48;
  I64 box_w=w-48;
  I64 box_h=16;

  // URL field.
  I64 url_bg=DGRAY;
  if (g_focus==0) url_bg=BLUE;
  FillRect(box_x,y0+32,box_w,box_h, url_bg);
  Text(box_x+8,y0+32,WHITE,url_bg, g_url);

  // Path field.
  I64 path_bg=DGRAY;
  if (g_focus==1) path_bg=BLUE;
  FillRect(box_x,y0+56,box_w,box_h, path_bg);
  Text(box_x+8,y0+56,WHITE,path_bg, g_path);

  // Cmd field.
  I64 cmd_bg=DGRAY;
  if (g_focus==2) cmd_bg=BLUE;
  FillRect(box_x,y0+80,box_w,box_h, cmd_bg);
  Text(box_x+8,y0+80,WHITE,cmd_bg, g_cmd);

  // Status.
  Text(x0,y0+112,YELLOW,bg,"Status:");
  FillRect(x0+72,y0+112,w-72,16,BLACK);
  Text(x0+72,y0+112,WHITE,bg,g_status);

  Present;
}

U0 DoHelp()
{
  Clear(BLACK);
  FillRect(0,0,SCR_W,8,BLUE);
  Text(8,0,WHITE,BLUE,"LinuxBridge Help");

  Text(16,24,WHITE,BLACK,"Browse: uses xdg-open on a URL (opens Chrome/Firefox/etc).");
  Text(16,32,WHITE,BLACK,"Open:   uses xdg-open on a file/dir path.");
  Text(16,40,WHITE,BLACK,"Run:    spawns a Linux command from an allowlist.");

  Text(16,56,LTGREEN,BLACK,"LinuxRun allowlist (deny-by-default):");
  Text(16,64,LTCYAN,BLACK,"- Env: TEMPLE_LINUX_RUN_ALLOW=\"chrome,firefox,foot\"");
  Text(16,72,LTCYAN,BLACK,"- Or file: /Cfg/LinuxRunAllow.txt (one program per line)");

  Text(16,88,YELLOW,BLACK,"Notes:");
  Text(16,96,WHITE,BLACK,"- /Home/... maps to TEMPLE_ROOT/Home/...");
  Text(16,104,WHITE,BLACK,"- ::/Demo/... maps to vendored TempleOS tree.");

  Text(16,120,LTGREEN,BLACK,"Press any key to return.");
  PressAKey;
}

U0 PromptAndBrowse()
{
  U8 *st=GetStr("URL:",g_url);
  if (st) {
    g_url=st;
    if (LinuxBrowse(st))
      g_status="Launched (browse)";
    else
      g_status=LinuxLastErr();
  }
}

U0 PromptAndOpen()
{
  U8 *st=GetStr("Path:",g_path);
  if (st) {
    g_path=st;
    if (LinuxOpen(st))
      g_status="Launched (open)";
    else
      g_status=LinuxLastErr();
  }
}

U0 PromptAndRun()
{
  U8 *st=GetStr("Cmd:",g_cmd);
  if (st) {
    g_cmd=st;
    if (LinuxRun(st))
      g_status="Launched (run)";
    else
      g_status=LinuxLastErr();
  }
}

U0 Main()
{
  I64 msg_code,arg1,arg2;

  MenuPush(
    "Linux {"
    "  Browse(MSG_CMD,CMD_BROWSE);"
    "  Open(MSG_CMD,CMD_OPEN);"
    "  Run(MSG_CMD,CMD_RUN);"
    "}"
    "Help {"
    "  Help(MSG_CMD,CMD_HELP);"
    "}"
    "File {"
    "  Exit(MSG_CMD,CMD_EXIT);"
    "}");

  DrawUI;

  while (TRUE) {
    msg_code=GetMsg(&arg1,&arg2,1<<MSG_KEY_DOWN+1<<MSG_CMD);

    if (msg_code==MSG_CMD) {
      if (arg1==CMD_BROWSE) {
        PromptAndBrowse;
      } else if (arg1==CMD_OPEN) {
        PromptAndOpen;
      } else if (arg1==CMD_RUN) {
        PromptAndRun;
      } else if (arg1==CMD_HELP) {
        DoHelp;
      } else if (arg1==CMD_EXIT) {
        break;
      }
      DrawUI;
      continue;
    }

    if (msg_code==MSG_KEY_DOWN) {
      // arg1 = ASCII
      if (arg1==CH_ESC || arg1==CH_SHIFT_ESC) {
        break;
      } else if (arg1=='\t') {
        g_focus=(g_focus+1)%3;
        DrawUI;
      } else if (arg1=='\n') {
        if (g_focus==0)
          PromptAndBrowse;
        else if (g_focus==1)
          PromptAndOpen;
        else
          PromptAndRun;
        DrawUI;
      } else if (arg1==0) {
        // F1 via scan code.
        if (arg2.u8[0]==SC_F1) {
          DoHelp;
          DrawUI;
        }
      }
    }
  }

  MenuPop;
}
