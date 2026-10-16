// EVX VM 명령어 표(0x447394, 256개)의 처리 함수를 함수로 만들고 opcode별로 디컴파일 덤프
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import java.io.*;
import java.util.*;

public class VmHandlers extends GhidraScript {
  @Override public void run() throws Exception {
    String out = getScriptArgs()[0];
    Address table = toAddr(0x447394);
    Map<Long, List<Integer>> byTarget = new TreeMap<>();
    long[] targets = new long[256];
    for (int op = 0; op < 256; op++) {
      long thunk = getInt(table.add(op * 4)) & 0xffffffffL;
      Address ta = toAddr(thunk);
      byte b = getByte(ta);
      long tgt = thunk;
      if ((b & 0xff) == 0xE9) tgt = thunk + 5 + getInt(ta.add(1));
      targets[op] = tgt;
      byTarget.computeIfAbsent(tgt, k -> new ArrayList<>()).add(op);
    }
    for (long t : byTarget.keySet()) {
      Address a = toAddr(t);
      if (getInstructionAt(a) == null) new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
      if (getFunctionAt(a) == null) {
        Function f = getFunctionContaining(a);
        if (f == null || !f.getEntryPoint().equals(a)) new CreateFunctionCmd(a).applyTo(currentProgram, monitor);
      }
    }
    DecompInterface di = new DecompInterface();
    di.openProgram(currentProgram);
    try (PrintWriter w = new PrintWriter(new FileWriter(out + "/vm_handlers.c"))) {
      for (Map.Entry<Long, List<Integer>> e : byTarget.entrySet()) {
        Function f = getFunctionAt(toAddr(e.getKey()));
        StringBuilder ops = new StringBuilder();
        for (int op : e.getValue()) ops.append(String.format("%02X ", op));
        w.println("// ===== opcodes [" + ops.toString().trim() + "] -> " + String.format("%08x", e.getKey()));
        if (f == null) { w.println("// no function"); continue; }
        DecompileResults r = di.decompileFunction(f, 60, monitor);
        w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed " + r.getErrorMessage());
      }
    }
    try (PrintWriter w = new PrintWriter(new FileWriter(out + "/vm_table.txt"))) {
      for (int op = 0; op < 256; op++) w.println(String.format("%02X %08x", op, targets[op]));
    }
  }
}
