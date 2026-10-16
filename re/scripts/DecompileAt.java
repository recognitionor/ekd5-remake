// 주어진 주소들(16진, 쉼표 구분)에 함수를 만들고 디컴파일해 출력 파일에 쓴다.
// 인자: <출력 파일> <주소,주소,...>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import java.io.*;

public class DecompileAt extends GhidraScript {
  @Override public void run() throws Exception {
    String[] a = getScriptArgs();
    DecompInterface di = new DecompInterface();
    di.openProgram(currentProgram);
    try (PrintWriter w = new PrintWriter(new FileWriter(a[0]))) {
      for (String s : a[1].split(",")) {
        Address ad = toAddr(Long.parseLong(s.trim(), 16));
        if (getInstructionAt(ad) == null) new DisassembleCommand(ad, null, true).applyTo(currentProgram, monitor);
        Function f = getFunctionAt(ad);
        if (f == null) {
          Function c = getFunctionContaining(ad);
          if (c != null && !c.getEntryPoint().equals(ad)) {
            // 기존 함수 본문에서 떼어내기 위해 새 함수 생성
          }
          new CreateFunctionCmd(ad).applyTo(currentProgram, monitor);
          f = getFunctionAt(ad);
        }
        w.println("// ===== " + s + (f == null ? " (함수 생성 실패)" : " " + f.getName()));
        if (f == null) continue;
        DecompileResults r = di.decompileFunction(f, 120, monitor);
        w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed " + r.getErrorMessage());
      }
    }
  }
}
