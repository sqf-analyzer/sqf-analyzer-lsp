# sqf-analyzer

This extension provides support for the [SQF programming language](https://community.bistudio.com/wiki/SQF_Syntax).

## Quick start

Install the extension and open an `.sqf` file.

## Features

- go to definition
- inlay hints for types and parameter names
- semantic syntax highlighting
- External Addons

### Full support for evaluating preprocessor

This extension supports preprocessor and SQF. For example,

```sqf
if a then {
    b
#ifdef A
};
#else
} else {
    c
};
#endif
d
```

and

```sqf
#define DOUBLES(var1,var2) ##var1##_##var2
#define QUOTE(var1) #var1
#define NAME(func) QUOTE(a\DOUBLES(fnc,func).sqf)

a = NAME(a)
```

It is tested on the complete source code of the
[official Antistasi](https://github.com/official-antistasi-community/A3-Antistasi). Furthermore, it has a line coverage of ~90%.

### Support for `CfgFunctions` in `config.cpp` and `description.ext`

This extension identifies the presence of `config.cpp` and `description.ext` to show function signatures and go to definition.

### Type inference

This analyzer has the set of operators supported by Arma 3 and will interpret the code
accordingly. For example, it can identify errors such as

```sqf
params [[\"_a\", true, [true]]]

private _b = _a + 1;
```

(`_a` is a boolean, 1 is a number, which cannot be added).
