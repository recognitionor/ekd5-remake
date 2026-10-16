// 모든 함수를 디컴파일해 하나의 텍스트로 덤프 + 문자열/임포트 목록
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.StringDataInstance;
import java.io.*;

public class DumpAll extends GhidraScript {
  @Override public void run() throws Exception {
    String outDir = getScriptArgs()[0];
    DecompInterface di = new DecompInterface();
    di.openProgram(currentProgram);
    try (PrintWriter w = new PrintWriter(new FileWriter(outDir + "/decompiled.c"))) {
      for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
        DecompileResults r = di.decompileFunction(f, 60, monitor);
        w.println("// ===== " + f.getName() + " @ " + f.getEntryPoint());
        w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed: " + r.getErrorMessage());
      }
    }
    try (PrintWriter w = new PrintWriter(new FileWriter(outDir + "/strings.txt"))) {
      for (Data d : currentProgram.getListing().getDefinedData(true)) {
        if (d.hasStringValue()) {
          StringBuilder refs = new StringBuilder();
          for (Reference ref : getReferencesTo(d.getAddress())) {
            Function f = getFunctionContaining(ref.getFromAddress());
            refs.append(f == null ? ref.getFromAddress().toString() : f.getName()).append(' ');
          }
          w.println(d.getAddress() + "\t" + StringDataInstance.getStringDataInstance(d).getStringValue().replace("\n","\\n") + "\t<- " + refs);
        }
      }
    }
    try (PrintWriter w = new PrintWriter(new FileWriter(outDir + "/imports.txt"))) {
      for (Symbol s : currentProgram.getSymbolTable().getExternalSymbols()) w.println(s.getParentNamespace() + "::" + s.getName());
    }
  }
}
