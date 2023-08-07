class CfgPatches {
    class ADDON {
        name = "DICT";
        units[] = {};
        weapons[] = {};
        requiredVersion = 1.60;
		author = "Golias";
		url = "https://github.com/LordGolias/dict";
    };
};

class CfgFunctions {
    class DICT {
        class common {
            class func {
                file = "test/fn_basic.sqf"
            }
        }
    }
}
