# Vendored RustJava

Copied from jjongjjongs/RustJava at `b3cb0ab` (a fork of dlunch/RustJava), so the
JVM can be fixed in the same commit as the emulator behaviour that needs it.

Changes made here since the copy:

- A field is told apart by the class that declares it as well as its name and
  type, and `getfield`/`putfield` look it up from the class the instruction
  names. A subclass field with a superclass field's name and type was the same
  field before. Covered by `test_data/src/FieldShadowing.java`.
