#include "pugixml.hpp"
#include <cstdio>
#include <string>

// Deterministic documents exercise parsing, entities, UTF-8, traversal and XPath.
int main() {
    unsigned total = 0;
    for (unsigned round = 0; round != 64; ++round) {
        std::string xml = "<?xml version='1.0'?><catalog><!--pilot-->";
        for (unsigned i = 0; i != 32; ++i) {
            unsigned id = round * 32 + i;
            xml += "<item id='" + std::to_string(id) + "' enabled='";
            xml += (i % 3 == 0 ? "yes" : "no");
            xml += "'><name>caf\xc3\xa9 &amp; tea</name><price>";
            xml += std::to_string(id + 1) + "</price></item>";
        }
        xml += "</catalog>";
        pugi::xml_document doc;
        if (!doc.load_buffer(xml.data(), xml.size())) return 10;
        unsigned count = 0, ids = 0;
        for (pugi::xml_node item: doc.child("catalog").children("item")) {
            ++count;
            ids += item.attribute("id").as_uint();
            if (std::string(item.child_value("name")) != "caf\xc3\xa9 & tea") return 11;
        }
        pugi::xpath_node_set selected = doc.select_nodes("/catalog/item[@enabled='yes']");
        double sum = doc.select_node("/catalog").node().select_nodes("item").size();
        if (sum != 32) return 12;
        pugi::xpath_query query("sum(/catalog/item[@enabled='yes']/price)");
        unsigned prices = static_cast<unsigned>(query.evaluate_number(doc));
        if (count != 32 || selected.size() != 11 || prices != 11 * (round * 32 + 1) + 165) return 13;
        pugi::xml_document invalid;
        if (invalid.load_string("<root><broken></root>")) return 14;
        total += ids + prices;
        std::printf("round=%u nodes=%u ids=%u selected=%u prices=%u\n", round, count, ids,
                    static_cast<unsigned>(selected.size()), prices);
    }
    std::printf("documents=64 nodes=2048 total=%u\n", total);
}
