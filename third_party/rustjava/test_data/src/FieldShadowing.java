// A subclass that declares a field with the same name and type as one its
// superclass declares. The instance holds both, and each class's code reads
// its own: 타워오브바벨3's dialogue box keeps its height in the base class's
// `d` and a scroll position in its own `d`.
class FieldShadowing {
    static class Base {
        public int d;
        public int e;

        void setBase(int d) {
            this.d = d;
        }

        int baseD() {
            return d;
        }
    }

    static class Derived extends Base {
        public int d;

        void setDerived(int d) {
            this.d = d;
        }

        int derivedD() {
            return d;
        }

        int superD() {
            return super.d;
        }
    }

    public static void main(String[] args) {
        Derived derived = new Derived();
        derived.setBase(62);
        derived.setDerived(0);
        derived.e = 7;

        System.out.println(derived.baseD());
        System.out.println(derived.derivedD());
        System.out.println(derived.superD());
        System.out.println(((Base) derived).d);
        System.out.println(derived.d);
        System.out.println(derived.e);
    }
}
