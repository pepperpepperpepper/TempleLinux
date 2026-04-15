// TempleLinux: FileBridge — copy files/dirs between Temple root and the Linux host.
//
// TempleLinux-specific built-ins used:
//   LinuxCopyToHost("temple_src","host_dst")    -> 1 on success, 0 on failure
//   LinuxCopyFromHost("host_src","temple_dst")  -> 1 on success, 0 on failure
//   LinuxLastErr()                              -> string
//   GetStr(msg,dft,flags)                       -> string
//
// Notes:
// - Host paths accept "~" / "~/..." for $HOME.
// - Host paths are restricted to an allowlist (default: $HOME):
//     - Env: TEMPLE_LINUX_FILE_ALLOW="/home/user:/tmp"
//     - Or file: /Cfg/LinuxFileAllow.txt (one path per line)
//     - Use "*" to allow any host path.

#define CMD_COPY_TO_HOST   1
#define CMD_COPY_FROM_HOST 2
#define CMD_SET_TEMPLE     3
#define CMD_SET_HOST       4
#define CMD_HELP           5
#define CMD_EXIT           6

U8 *g_temple="/Home/";
U8 *g_host="~/Downloads";
U8 *g_status="";
I64 g_focus=0; // 0=temple path, 1=host path

U0 DrawUI()
{
  I64 fg=WHITE,bg=BLACK;
  I64 x0=16;
  I64 y0=24;
  I64 w=SCR_W-32;

  Clear(BLACK);

  FillRect(0,0,SCR_W,8,BLUE);
  Text(8,0,WHITE,BLUE,"FileBridge");

  Text(x0,y0,LTGREEN,bg,"Tab:Next  Enter:Edit  F5:Temple->Host  F6:Host->Temple  Esc:Exit");
  Text(x0,y0+8,LTCYAN,bg,"Menu: File { Copy... }  Edit { Paths... }  Help { Help }");

  Text(x0,y0+32,WHITE,bg,"Temple path:");
  Text(x0,y0+56,WHITE,bg,"Host path:");

  I64 box_x=x0+104;
  I64 box_w=w-104;
  I64 box_h=16;

  I64 temple_bg=DGRAY;
  if (g_focus==0) temple_bg=BLUE;
  FillRect(box_x,y0+32,box_w,box_h, temple_bg);
  Text(box_x+8,y0+32,WHITE,temple_bg, g_temple);

  I64 host_bg=DGRAY;
  if (g_focus==1) host_bg=BLUE;
  FillRect(box_x,y0+56,box_w,box_h, host_bg);
  Text(box_x+8,y0+56,WHITE,host_bg, g_host);

  Text(x0,y0+88,YELLOW,bg,"Status:");
  FillRect(x0+72,y0+88,w-72,16,BLACK);
  Text(x0+72,y0+88,WHITE,bg,g_status);

  Present;
}

U0 DoHelp()
{
  Clear(BLACK);
  FillRect(0,0,SCR_W,8,BLUE);
  Text(8,0,WHITE,BLUE,"FileBridge Help");

  Text(16,24,WHITE,BLACK,"F5: Copy from Temple path -> Host path");
  Text(16,32,WHITE,BLACK,"F6: Copy from Host path -> Temple path");
  Text(16,48,WHITE,BLACK,"If the destination path is an existing directory,");
  Text(16,56,WHITE,BLACK,"FileBridge copies into it (like cp).");

  Text(16,72,LTGREEN,BLACK,"Host path allowlist (default: $HOME):");
  Text(16,80,LTCYAN,BLACK,"- Env: TEMPLE_LINUX_FILE_ALLOW=\"/home/user:/tmp\"");
  Text(16,88,LTCYAN,BLACK,"- Or file: /Cfg/LinuxFileAllow.txt (one path per line)");
  Text(16,96,LTCYAN,BLACK,"- Use \"*\" to allow any host path");

  Text(16,112,YELLOW,BLACK,"Temple path notes:");
  Text(16,120,WHITE,BLACK,"- /Home/... maps to TEMPLE_ROOT/Home/...");
  Text(16,128,WHITE,BLACK,"- ::/Demo/... maps to the vendored TempleOS tree (read-only).");

  Text(16,144,LTGREEN,BLACK,"Press any key to return.");
  PressAKey;
}

U0 PromptTemple()
{
  U8 *st=GetStr("Temple path:",g_temple);
  if (st) g_temple=st;
}

U0 PromptHost()
{
  U8 *st=GetStr("Host path:",g_host);
  if (st) g_host=st;
}

U0 CopyTempleToHost()
{
  if (LinuxCopyToHost(g_temple, g_host))
    g_status="Copied Temple -> Host";
  else
    g_status=LinuxLastErr();
}

U0 CopyHostToTemple()
{
  if (LinuxCopyFromHost(g_host, g_temple))
    g_status="Copied Host -> Temple";
  else
    g_status=LinuxLastErr();
}

U0 Main()
{
  I64 msg_code,arg1,arg2;

  MenuPush(
    "File {"
    "  TempleToHost(MSG_CMD,CMD_COPY_TO_HOST);"
    "  HostToTemple(MSG_CMD,CMD_COPY_FROM_HOST);"
    "  Exit(MSG_CMD,CMD_EXIT);"
    "}"
    "Edit {"
    "  TemplePath(MSG_CMD,CMD_SET_TEMPLE);"
    "  HostPath(MSG_CMD,CMD_SET_HOST);"
    "}"
    "Help {"
    "  Help(MSG_CMD,CMD_HELP);"
    "}");

  DrawUI;

  while (TRUE) {
    msg_code=GetMsg(&arg1,&arg2,1<<MSG_KEY_DOWN+1<<MSG_CMD);

    if (msg_code==MSG_CMD) {
      if (arg1==CMD_COPY_TO_HOST) {
        CopyTempleToHost;
      } else if (arg1==CMD_COPY_FROM_HOST) {
        CopyHostToTemple;
      } else if (arg1==CMD_SET_TEMPLE) {
        PromptTemple;
      } else if (arg1==CMD_SET_HOST) {
        PromptHost;
      } else if (arg1==CMD_HELP) {
        DoHelp;
      } else if (arg1==CMD_EXIT) {
        break;
      }
      DrawUI;
      continue;
    }

    if (msg_code==MSG_KEY_DOWN) {
      if (arg1==CH_ESC || arg1==CH_SHIFT_ESC) {
        break;
      } else if (arg1=='\t') {
        g_focus=(g_focus+1)%2;
        DrawUI;
      } else if (arg1=='\n') {
        if (g_focus==0)
          PromptTemple;
        else
          PromptHost;
        DrawUI;
      } else if (arg1==0) {
        if (arg2.u8[0]==SC_F1) {
          DoHelp;
          DrawUI;
        } else if (arg2.u8[0]==SC_F5) {
          CopyTempleToHost;
          DrawUI;
        } else if (arg2.u8[0]==SC_F6) {
          CopyHostToTemple;
          DrawUI;
        }
      }
    }
  }

  MenuPop;
}

