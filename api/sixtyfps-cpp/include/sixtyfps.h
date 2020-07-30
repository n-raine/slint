#pragma once

#include <vector>
#include <memory>

namespace sixtyfps::internal {
// Workaround https://github.com/eqrion/cbindgen/issues/43
struct ComponentVTable;
struct ItemVTable;
}
#include "sixtyfps_internal.h"
#include "sixtyfps_gl_internal.h"

namespace sixtyfps {

extern "C" {
extern const internal::ItemVTable RectangleVTable;
extern const internal::ItemVTable BorderRectangleVTable;
extern const internal::ItemVTable TextVTable;
extern const internal::ItemVTable TouchAreaVTable;
extern const internal::ItemVTable ImageVTable;
extern const internal::ItemVTable PathVTable;
extern const internal::ItemVTable FlickableVTable;
}

// Bring opaque structure in scope
using internal::ComponentVTable;
using internal::ItemTreeNode;
using ComponentRef = VRef<ComponentVTable>;
using ItemVisitorRefMut = VRefMut<internal::ItemVisitorVTable>;
using internal::WindowProperties;
using internal::EasingCurve;

struct ComponentWindow
{
    ComponentWindow() { internal::sixtyfps_component_window_gl_renderer_init(&inner); }
    ~ComponentWindow() { internal::sixtyfps_component_window_drop(&inner); }
    ComponentWindow(const ComponentWindow &) = delete;
    ComponentWindow(ComponentWindow &&) = delete;
    ComponentWindow &operator=(const ComponentWindow &) = delete;

    template<typename Component>
    void run(Component *c)
    {
        auto props = c->window_properties();
        sixtyfps_component_window_run(
                &inner, VRefMut<ComponentVTable> { &Component::component_type, c }, &props);
    }

private:
    internal::ComponentWindowOpaque inner;
};

using internal::BorderRectangle;
using internal::Image;
using internal::Path;
using internal::Rectangle;
using internal::Text;
using internal::TouchArea;
using internal::Flickable;

constexpr inline ItemTreeNode<uint8_t> make_item_node(std::uintptr_t offset,
                                                      const internal::ItemVTable *vtable,
                                                      uint32_t child_count, uint32_t child_index)
{
    return ItemTreeNode<uint8_t> { ItemTreeNode<uint8_t>::Item_Body {
            ItemTreeNode<uint8_t>::Tag::Item, { vtable, offset }, child_count, child_index } };
}

constexpr inline ItemTreeNode<uint8_t> make_dyn_node(std::uintptr_t offset)
{
    return ItemTreeNode<uint8_t> { ItemTreeNode<uint8_t>::DynamicTree_Body {
            ItemTreeNode<uint8_t>::Tag::DynamicTree, offset } };
}

using internal::sixtyfps_visit_item_tree;

// layouts:
using internal::GridLayoutCellData;
using internal::GridLayoutData;
using internal::PathLayoutData;
using internal::PathLayoutItemData;
using internal::Slice;
using internal::solve_grid_layout;
using internal::solve_path_layout;

// models

struct Model
{
    virtual ~Model() = default;
    Model() = default;
    Model(const Model &) = delete;
    Model &operator=(const Model &) = delete;
    virtual int count() const = 0;
    virtual const void *get(int i) const = 0;
};

template<int Count, typename ModelData>
struct ArrayModel : Model
{
    std::array<ModelData, Count> data;
    template<typename... A>
    ArrayModel(A &&... a) : data { std::forward<A>(a)... }
    {
    }
    ArrayModel(int x) { }
    int count() const override { return Count; }
    const void *get(int i) const override { return &data[i]; }
};

struct IntModel : Model
{
    IntModel(int d) : data(d) { }
    int data;
    int count() const override { return data; }
    const void *get(int) const override { return &data; }
};

template<typename C>
struct Repeater
{
    std::vector<std::unique_ptr<C>> data;

    template<typename Parent>
    void update_model(Model *model, const Parent *parent) const
    {
        auto &data = const_cast<Repeater *>(this)->data;
        data.clear();
        auto count = model->count();
        for (auto i = 0; i < count; ++i) {
            auto x = std::make_unique<C>();
            x->parent = parent;
            x->update_data(i, model->get(i));
            data.push_back(std::move(x));
        }
    }

    void visit(ItemVisitorRefMut visitor) const
    {
        for (const auto &x : data) {
            VRef<ComponentVTable> ref { &C::component_type, x.get() };
            ref.vtable->visit_children_item(ref, -1, visitor);
        }
    }
};

Flickable::Flickable() { sixtyfps_flickable_data_init(&data); }
Flickable::~Flickable() { sixtyfps_flickable_data_free(&data); }

} // namespace sixtyfps
