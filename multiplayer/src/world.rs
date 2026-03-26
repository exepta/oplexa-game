use naia_shared::{
    BigMap, BigMapKey, ComponentFieldUpdate, ComponentKind, ComponentUpdate,
    GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter, ReplicaDynMutWrapper,
    ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper, ReplicaRefTrait, ReplicaRefWrapper,
    Replicate, SerdeErr, WorldMutType, WorldRefType,
};
use std::{any::Any, collections::HashMap};

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct NetworkEntity(u64);

impl BigMapKey for NetworkEntity {
    fn to_u64(&self) -> u64 {
        self.0
    }

    fn from_u64(value: u64) -> Self {
        Self(value)
    }
}

pub struct NetworkWorld {
    entities: BigMap<NetworkEntity, HashMap<ComponentKind, Box<dyn Replicate>>>,
}

impl Default for NetworkWorld {
    fn default() -> Self {
        Self {
            entities: BigMap::new(),
        }
    }
}

impl NetworkWorld {
    pub fn proxy(&self) -> NetworkWorldRef<'_> {
        NetworkWorldRef { world: self }
    }

    pub fn proxy_mut(&mut self) -> NetworkWorldMut<'_> {
        NetworkWorldMut { world: self }
    }
}

pub struct NetworkWorldRef<'w> {
    world: &'w NetworkWorld,
}

pub struct NetworkWorldMut<'w> {
    world: &'w mut NetworkWorld,
}

impl WorldRefType<NetworkEntity> for NetworkWorldRef<'_> {
    fn has_entity(&self, entity: &NetworkEntity) -> bool {
        self.world.entities.contains_key(entity)
    }

    fn entities(&self) -> Vec<NetworkEntity> {
        self.world.entities.iter().map(|(entity, _)| entity).collect()
    }

    fn has_component<R: Replicate>(&self, entity: &NetworkEntity) -> bool {
        self.world
            .entities
            .get(entity)
            .is_some_and(|components| components.contains_key(&ComponentKind::of::<R>()))
    }

    fn has_component_of_kind(
        &self,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        self.world
            .entities
            .get(entity)
            .is_some_and(|components| components.contains_key(component_kind))
    }

    fn component<R: Replicate>(&self, entity: &NetworkEntity) -> Option<ReplicaRefWrapper<'_, R>> {
        let components = self.world.entities.get(entity)?;
        let component = components.get(&ComponentKind::of::<R>())?;
        let raw_ref = component.to_any().downcast_ref::<R>()?;
        Some(ReplicaRefWrapper::new(ComponentRef::new(raw_ref)))
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        let components = self.world.entities.get(entity)?;
        let component = components.get(component_kind)?;
        Some(ReplicaDynRefWrapper::new(component.dyn_ref()))
    }
}

impl WorldRefType<NetworkEntity> for NetworkWorldMut<'_> {
    fn has_entity(&self, entity: &NetworkEntity) -> bool {
        self.world.entities.contains_key(entity)
    }

    fn entities(&self) -> Vec<NetworkEntity> {
        self.world.entities.iter().map(|(entity, _)| entity).collect()
    }

    fn has_component<R: Replicate>(&self, entity: &NetworkEntity) -> bool {
        self.world
            .entities
            .get(entity)
            .is_some_and(|components| components.contains_key(&ComponentKind::of::<R>()))
    }

    fn has_component_of_kind(
        &self,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        self.world
            .entities
            .get(entity)
            .is_some_and(|components| components.contains_key(component_kind))
    }

    fn component<R: Replicate>(&self, entity: &NetworkEntity) -> Option<ReplicaRefWrapper<'_, R>> {
        let components = self.world.entities.get(entity)?;
        let component = components.get(&ComponentKind::of::<R>())?;
        let raw_ref = component.to_any().downcast_ref::<R>()?;
        Some(ReplicaRefWrapper::new(ComponentRef::new(raw_ref)))
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        let components = self.world.entities.get(entity)?;
        let component = components.get(component_kind)?;
        Some(ReplicaDynRefWrapper::new(component.dyn_ref()))
    }
}

impl WorldMutType<NetworkEntity> for NetworkWorldMut<'_> {
    fn spawn_entity(&mut self) -> NetworkEntity {
        self.world.entities.insert(HashMap::new())
    }

    fn local_duplicate_entity(&mut self, entity: &NetworkEntity) -> NetworkEntity {
        let new_entity = self.spawn_entity();
        self.local_duplicate_components(&new_entity, entity);
        new_entity
    }

    fn local_duplicate_components(
        &mut self,
        mutable_entity: &NetworkEntity,
        immutable_entity: &NetworkEntity,
    ) {
        for component_kind in self.component_kinds(immutable_entity) {
            let boxed_component = self
                .component_of_kind(immutable_entity, &component_kind)
                .map(|component| component.copy_to_box())
                .expect("Component kind vanished while duplicating");
            self.insert_boxed_component(mutable_entity, boxed_component);
        }
    }

    fn despawn_entity(&mut self, entity: &NetworkEntity) {
        self.world.entities.remove(entity);
    }

    fn component_kinds(&mut self, entity: &NetworkEntity) -> Vec<ComponentKind> {
        self.world
            .entities
            .get(entity)
            .map(|components| components.keys().copied().collect())
            .unwrap_or_default()
    }

    fn component_mut<R: Replicate>(
        &mut self,
        entity: &NetworkEntity,
    ) -> Option<ReplicaMutWrapper<'_, R>> {
        let components = self.world.entities.get_mut(entity)?;
        let component = components.get_mut(&ComponentKind::of::<R>())?;
        let raw_ref = component.to_any_mut().downcast_mut::<R>()?;
        Some(ReplicaMutWrapper::new(ComponentMut::new(raw_ref)))
    }

    fn component_mut_of_kind<'a>(
        &'a mut self,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'a>> {
        let components = self.world.entities.get_mut(entity)?;
        let component = components.get_mut(component_kind)?;
        Some(ReplicaDynMutWrapper::new(component.dyn_mut()))
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        if let Some(mut component) = self.component_mut_of_kind(entity, component_kind) {
            component.read_apply_update(converter, update)?;
        }

        Ok(())
    }

    fn component_apply_field_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
        update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {
        if let Some(mut component) = self.component_mut_of_kind(entity, component_kind) {
            let _ = component.read_apply_field_update(converter, update);
        }

        Ok(())
    }

    fn mirror_entities(&mut self, mutable_entity: &NetworkEntity, immutable_entity: &NetworkEntity) {
        for component_kind in self.component_kinds(immutable_entity) {
            self.mirror_components(mutable_entity, immutable_entity, &component_kind);
        }
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &NetworkEntity,
        immutable_entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) {
        let immutable_component = self
            .world
            .entities
            .get(immutable_entity)
            .and_then(|components| components.get(component_kind))
            .map(|component| component.copy_to_box());

        if let Some(immutable_component) = immutable_component {
            if let Some(mutable_component) = self
                .world
                .entities
                .get_mut(mutable_entity)
                .and_then(|components| components.get_mut(component_kind))
            {
                mutable_component.mirror(immutable_component.as_ref());
            }
        }
    }

    fn insert_component<R: Replicate>(&mut self, entity: &NetworkEntity, component: R) {
        let components = self
            .world
            .entities
            .get_mut(entity)
            .expect("Entity must exist before inserting component");
        let component_kind = ComponentKind::of::<R>();
        components.insert(component_kind, Box::new(component));
    }

    fn insert_boxed_component(
        &mut self,
        entity: &NetworkEntity,
        component: Box<dyn Replicate>,
    ) {
        let component_kind = component.kind();
        let components = self
            .world
            .entities
            .get_mut(entity)
            .expect("Entity must exist before inserting component");
        components.insert(component_kind, component);
    }

    fn remove_component<R: Replicate>(&mut self, entity: &NetworkEntity) -> Option<R> {
        let components = self.world.entities.get_mut(entity)?;
        let component = components.remove(&ComponentKind::of::<R>())?;
        Box::<dyn Any>::downcast::<R>(component.to_boxed_any())
            .ok()
            .map(|component| *component)
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        self.world
            .entities
            .get_mut(entity)
            .and_then(|components| components.remove(component_kind))
    }

    fn entity_publish(
        &mut self,
        global_world_manager: &dyn GlobalWorldManagerType<NetworkEntity>,
        entity: &NetworkEntity,
    ) {
        for component_kind in self.component_kinds(entity) {
            self.component_publish(global_world_manager, entity, &component_kind);
        }
    }

    fn component_publish(
        &mut self,
        global_world_manager: &dyn GlobalWorldManagerType<NetworkEntity>,
        entity: &NetworkEntity,
        component_kind: &ComponentKind,
    ) {
        if let Some(component) = self
            .world
            .entities
            .get_mut(entity)
            .and_then(|components| components.get_mut(component_kind))
        {
            let mutator = global_world_manager.register_component(
                entity,
                component_kind,
                component.diff_mask_size(),
            );
            component.publish(&mutator);
        }
    }

    fn entity_unpublish(&mut self, _entity: &NetworkEntity) {}

    fn component_unpublish(&mut self, _entity: &NetworkEntity, _component_kind: &ComponentKind) {}

    fn entity_enable_delegation(
        &mut self,
        _global_world_manager: &dyn GlobalWorldManagerType<NetworkEntity>,
        _entity: &NetworkEntity,
    ) {
    }

    fn component_enable_delegation(
        &mut self,
        _global_world_manager: &dyn GlobalWorldManagerType<NetworkEntity>,
        _entity: &NetworkEntity,
        _component_kind: &ComponentKind,
    ) {
    }

    fn entity_disable_delegation(&mut self, _entity: &NetworkEntity) {}

    fn component_disable_delegation(
        &mut self,
        _entity: &NetworkEntity,
        _component_kind: &ComponentKind,
    ) {
    }
}

struct ComponentRef<'a, R: Replicate> {
    inner: &'a R,
}

impl<'a, R: Replicate> ComponentRef<'a, R> {
    fn new(inner: &'a R) -> Self {
        Self { inner }
    }
}

impl<R: Replicate> ReplicaRefTrait<R> for ComponentRef<'_, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

struct ComponentMut<'a, R: Replicate> {
    inner: &'a mut R,
}

impl<'a, R: Replicate> ComponentMut<'a, R> {
    fn new(inner: &'a mut R) -> Self {
        Self { inner }
    }
}

impl<R: Replicate> ReplicaRefTrait<R> for ComponentMut<'_, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

impl<R: Replicate> ReplicaMutTrait<R> for ComponentMut<'_, R> {
    fn to_mut(&mut self) -> &mut R {
        self.inner
    }
}
